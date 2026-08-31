//! # `app-server`
//!
//! Runs the business modules behind one of several pluggable workers, selected
//! by the `WORK_MODE` environment variable: `grpc`, `internal_grpc`, `consumer`,
//! `cron`, or `openapi`.

mod worker;

use worker::WorkerArgs;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let worker = WorkerArgs::load_from_env()?;
    tracing::info!(?worker, "starting app-server worker");

    tokio::select! {
        result = worker.execute() => {
            result?;
        }
        _ = shutdown_signal() => {
            tracing::info!("shutdown signal received");
        }
    }
    Ok(())
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
