//! Liveness and readiness of this instance.
//!
//! - **Liveness**: the process answers. `GET /livez` on the health listener
//!   returns 200 whenever the runtime can serve a request.
//! - **Readiness**: the workers are started, no shutdown is in progress, and
//!   every shared dependency (PostgreSQL primary and replica, Redis, Iggy)
//!   answered its last probe. `GET /readyz` returns 200 or 503, and the gRPC
//!   listener serves the same answer as the overall (`""`) status of
//!   `grpc.health.v1.Health`.
//!
//! The dependencies are probed in the background every [`PROBE_INTERVAL`],
//! so a probe request never touches them.

use crate::dependencies::Dependencies;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use redis::aio::MultiplexedConnection;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tonic_health::ServingStatus;
use tonic_health::pb::health_server::{Health, HealthServer};
use wakuwaku::deps::iggy::prelude::{IggyClient, SystemClient};

/// Time between two probes of the dependencies.
pub const PROBE_INTERVAL: Duration = Duration::from_secs(5);
/// A dependency that takes longer to answer a probe is unavailable.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// The readiness of this instance. Cloning shares it.
#[derive(Clone)]
pub struct HealthCheck {
    pg: PgPool,
    /// `None` when reads go to the primary.
    pg_read: Option<PgPool>,
    redis: MultiplexedConnection,
    iggy: Arc<IggyClient>,
    ready: Arc<watch::Sender<bool>>,
}

impl HealthCheck {
    /// Not ready until [`watch`](Self::watch) runs.
    pub fn new(deps: &Dependencies) -> Self {
        Self {
            pg: deps.pg.clone(),
            pg_read: deps.has_read_replica.then(|| deps.pg_read.clone()),
            redis: deps.redis.clone(),
            iggy: deps.iggy.clone(),
            ready: Arc::new(watch::channel(false).0),
        }
    }

    /// Probe the dependencies until `shutdown` is cancelled, then report
    /// not ready. Call it once the workers are started.
    pub async fn watch(self, shutdown: CancellationToken) {
        let mut interval = tokio::time::interval(PROBE_INTERVAL);
        loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                _ = interval.tick() => {}
            }
            let failure = self.probe().await.err();
            let ready = failure.is_none();
            if self.ready.send_replace(ready) != ready {
                match failure {
                    None => tracing::info!("instance is ready"),
                    Some(ProbeFailure { dependency, error }) => {
                        tracing::warn!(dependency, error, "instance is not ready");
                    }
                }
            }
        }
        if self.ready.send_replace(false) {
            tracing::info!("instance is not ready: shutting down");
        }
    }

    /// The HTTP probes: `GET /livez` and `GET /readyz`.
    pub fn router(&self) -> Router {
        Router::new()
            .route("/livez", get(|| async { "live" }))
            .route("/readyz", get(readyz))
            .with_state(self.ready.subscribe())
    }

    /// `grpc.health.v1.Health`, reporting readiness as the overall status.
    pub fn grpc_service(&self) -> HealthServer<impl Health> {
        let (reporter, server) = tonic_health::server::health_reporter();
        let mut ready = self.ready.subscribe();
        tokio::spawn(async move {
            loop {
                let status = if *ready.borrow_and_update() {
                    ServingStatus::Serving
                } else {
                    ServingStatus::NotServing
                };
                reporter.set_service_status("", status).await;
                if ready.changed().await.is_err() {
                    break;
                }
            }
        });
        server
    }

    /// Probe every dependency at once, returning the first failure.
    async fn probe(&self) -> Result<(), ProbeFailure> {
        let pg = probe("postgresql", ping_pg(&self.pg));
        let pg_read = async {
            match &self.pg_read {
                Some(pool) => probe("postgresql replica", ping_pg(pool)).await,
                None => Ok(()),
            }
        };
        let mut redis = self.redis.clone();
        let ping = redis::cmd("PING");
        let redis = probe("redis", ping.query_async::<()>(&mut redis));
        let iggy = probe("iggy", self.iggy.ping());
        let (pg, pg_read, redis, iggy) = tokio::join!(pg, pg_read, redis, iggy);
        pg.and(pg_read).and(redis).and(iggy)
    }
}

struct ProbeFailure {
    dependency: &'static str,
    error: String,
}

async fn ping_pg(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT 1").execute(pool).await.map(drop)
}

/// Run one probe of `dependency`, bounded by [`PROBE_TIMEOUT`].
async fn probe<E: std::fmt::Display>(
    dependency: &'static str,
    check: impl Future<Output = Result<(), E>>,
) -> Result<(), ProbeFailure> {
    let error = match tokio::time::timeout(PROBE_TIMEOUT, check).await {
        Ok(Ok(())) => return Ok(()),
        Ok(Err(error)) => error.to_string(),
        Err(_) => format!("no answer within {PROBE_TIMEOUT:?}"),
    };
    Err(ProbeFailure { dependency, error })
}

async fn readyz(State(ready): State<watch::Receiver<bool>>) -> (StatusCode, &'static str) {
    if *ready.borrow() {
        (StatusCode::OK, "ready")
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "not ready")
    }
}
