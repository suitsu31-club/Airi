//! # `app-server`
//!
//! The main application server binary. It wires the business modules under
//! `modules/` together and runs them behind one or more *workers*.
//!
//! A worker is a single run mode selected at startup (`APP_MODE`):
//!
//! - `grpc`: a **gRPC** server exposing each module's `rpc` services,
//! - `consumer`: an **Iggy consumer** driving each module's event `hooks`,
//! - `cron`: a **cron executor** running each module's `CronJob`s,
//! - `all` (default): all three in one process.
//!
//! Shipping one binary that can run in several modes keeps deployment uniform
//! while letting each responsibility scale independently.
//!
//! See `bin/app-server/README.md` for the full description.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(clippy::arithmetic_side_effects)]

mod dependencies;
mod health_check;
mod observability;
mod settings;
pub mod worker;

use anyhow::Context;
use clap::Parser;
use dependencies::Dependencies;
use health_check::HealthCheck;
use settings::Settings;
use std::process::ExitCode;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> ExitCode {
    let settings = Settings::parse();
    if let Err(error) = observability::init(settings.log_format) {
        eprintln!("app-server: setting up logging: {error:#}");
        return ExitCode::FAILURE;
    }
    match run(settings).await {
        Ok(()) => {
            tracing::info!("app-server stopped");
            ExitCode::SUCCESS
        }
        Err(error) => {
            tracing::error!(error = format!("{error:#}"), "app-server failed");
            ExitCode::FAILURE
        }
    }
}

async fn run(settings: Settings) -> anyhow::Result<()> {
    let shutdown = CancellationToken::new();
    tokio::spawn(cancel_on_signal(shutdown.clone()));

    let deps = tokio::select! {
        deps = Dependencies::connect(&settings) => deps?,
        () = shutdown.cancelled() => return Ok(()),
    };

    let health = HealthCheck::new(&deps);
    let listener = TcpListener::bind(settings.health_listen)
        .await
        .with_context(|| {
            format!(
                "binding the health listener to {} (HEALTH_LISTEN)",
                settings.health_listen
            )
        })?;
    tracing::info!(listen = %settings.health_listen, "health probes listening");
    let probes = health.router();
    // Probes keep answering while the workers drain, until the process exits.
    tokio::spawn(async move {
        if let Err(error) = axum::serve(listener, probes).await {
            tracing::error!(%error, "the health probe server failed");
        }
    });

    worker::run(&settings, &deps, &health, shutdown).await
}

/// Cancel `shutdown` on SIGTERM or Ctrl-C.
async fn cancel_on_signal(shutdown: CancellationToken) {
    let ctrl_c = async {
        if let Err(error) = tokio::signal::ctrl_c().await {
            tracing::warn!(%error, "cannot listen for Ctrl-C");
            std::future::pending::<()>().await;
        }
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut terminate) => {
                terminate.recv().await;
            }
            Err(error) => {
                tracing::warn!(%error, "cannot listen for SIGTERM");
                std::future::pending::<()>().await;
            }
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    tracing::info!("shutdown signal received");
    shutdown.cancel();
}
