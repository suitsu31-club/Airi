//! The workers, and the registry they are built from.
//!
//! [`services`] connects the modules to the shared dependencies once. Each
//! worker clones that registry and registers its own components from it:
//!
//! | Worker       | Registers                                  | File          |
//! |--------------|--------------------------------------------|---------------|
//! | [`grpc`]     | each module's `rpc` servers                | `grpc.rs`     |
//! | [`consumer`] | each module's Iggy event hooks, per topic  | `consumer.rs` |
//! | [`cron`]     | each module's `CronJob`s                   | `cron.rs`     |
//!
//! Adding a module: register its services in [`services`] (and [`Services`]),
//! then add one registration call to each worker it has components for.

pub mod consumer;
pub mod cron;
pub mod grpc;

use crate::dependencies::Dependencies;
use crate::health_check::HealthCheck;
use crate::settings::Settings;
use anyhow::{Context, anyhow};
use base::services::config::ConfigLoader;
use clap::ValueEnum;
use mailer::services::delivery::MailDeliveryService;
use mailer::utils::secret_key::secret_key_from_env;
use std::sync::Arc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use wakuwaku::ServiceBuilder;
use wakuwaku::entity::redis::RedisSource;
use wakuwaku::entity::sqlx::backend::PgSource;
use wakuwaku::entity::{Db, ReadOnly, ReadWrite};
use wakuwaku::iggy::Publisher;

/// The workers a process runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    /// Serve the gRPC API.
    Grpc,
    /// Consume Iggy events.
    Consumer,
    /// Run cron jobs.
    Cron,
    /// Every worker in one process, for small deployments.
    All,
}

impl Mode {
    const fn runs_grpc(self) -> bool {
        matches!(self, Self::Grpc | Self::All)
    }

    const fn runs_consumer(self) -> bool {
        matches!(self, Self::Consumer | Self::All)
    }

    const fn runs_cron(self) -> bool {
        matches!(self, Self::Cron | Self::All)
    }
}

/// The type of a [`ServiceBuilder`] holding the listed services, most
/// recently registered first.
macro_rules! registry {
    ($first:ty $(,)?) => { ServiceBuilder<$first> };
    ($last:ty, $($earlier:ty),+ $(,)?) => { ServiceBuilder<$last, registry!($($earlier),+)> };
}

/// What [`services`] registers, most recent first. Keep it in step with the
/// chain there; the compiler rejects any difference.
pub type Services = registry![
    // mailer
    MailDeliveryService,
    // base
    ConfigLoader,
    // shared dependencies
    Publisher,
    Db<RedisSource, ReadWrite>,
    Db<PgSource, ReadOnly>,
    Db<PgSource, ReadWrite>,
];

/// Build every module's services from the shared dependencies.
pub fn services(deps: &Dependencies) -> anyhow::Result<Services> {
    let mailer_secret_key = secret_key_from_env()
        .context("reading MAILER_SECRET_KEY")?
        .map(Arc::new);

    let services = ServiceBuilder::new(Db::<PgSource, ReadWrite>::new(PgSource::new(
        deps.pg.clone(),
    )))
    .push(Db::<PgSource, ReadOnly>::new(PgSource::new(
        deps.pg_read.clone(),
    )))
    .push(Db::<RedisSource, ReadWrite>::new(RedisSource::new(
        deps.redis.clone(),
    )))
    .push(deps.publisher.clone())
    // base
    .compose_subservice(|s| {
        ConfigLoader::new(
            s.provide::<Db<PgSource, ReadOnly>, _>().clone(),
            s.provide::<Db<RedisSource, ReadWrite>, _>().clone(),
        )
    })
    // mailer
    .compose_subservice(|s| {
        MailDeliveryService::new(
            s.provide::<Db<PgSource, ReadWrite>, _>().clone(),
            s.provide::<Publisher, _>().clone(),
            s.provide::<ConfigLoader, _>().clone(),
            mailer_secret_key,
        )
    });
    Ok(services)
}

/// Start the workers of `settings.mode`, run until `shutdown` is cancelled
/// (or the gRPC server fails), then stop them within the shutdown timeout.
pub async fn run(
    settings: &Settings,
    deps: &Dependencies,
    health: &HealthCheck,
    shutdown: CancellationToken,
) -> anyhow::Result<()> {
    let services = services(deps)?;
    let mode = settings.mode;

    let cron = mode.runs_cron().then(|| cron::start(&services));
    let consumers = if mode.runs_consumer() {
        consumer::start(&services, deps, settings).await?
    } else {
        Vec::new()
    };
    let mut grpc = if mode.runs_grpc() {
        Some(grpc::spawn(&services, settings.grpc_listen, health, shutdown.clone())?)
    } else {
        None
    };
    tokio::spawn(health.clone().watch(shutdown.clone()));
    tracing::info!(?mode, "workers started");

    // The gRPC server only stops by itself when it fails.
    let failure = match grpc.as_mut() {
        Some(server) => tokio::select! {
            () = shutdown.cancelled() => None,
            result = server => Some(match result {
                Ok(Ok(())) => anyhow!("the gRPC server stopped"),
                Ok(Err(error)) => anyhow::Error::new(error).context("the gRPC server failed"),
                Err(error) => anyhow::Error::new(error).context("the gRPC server task failed"),
            }),
        },
        None => {
            shutdown.cancelled().await;
            None
        }
    };
    shutdown.cancel();
    if failure.is_some() {
        grpc = None;
    }

    tracing::info!("stopping workers");
    let stop = async {
        let mut stopping = JoinSet::new();
        for runtime in consumers {
            stopping.spawn(runtime.shutdown());
        }
        if let Some(runtime) = cron {
            stopping.spawn(runtime.shutdown());
        }
        let grpc = async {
            match grpc {
                Some(server) => server.await?.context("the gRPC server failed to stop"),
                None => Ok(()),
            }
        };
        let (grpc, ()) = tokio::join!(grpc, async {
            while stopping.join_next().await.is_some() {}
        });
        grpc
    };
    let stopped = match tokio::time::timeout(settings.shutdown_timeout(), stop).await {
        Ok(result) => result,
        Err(_) => {
            tracing::warn!(
                timeout = ?settings.shutdown_timeout(),
                "workers did not stop in time; cancelling the work in flight"
            );
            Ok(())
        }
    };
    match failure {
        Some(error) => Err(error),
        None => stopped,
    }
}
