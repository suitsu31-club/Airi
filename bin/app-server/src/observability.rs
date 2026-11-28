//! Logging: the global `tracing` subscriber.
//!
//! Events go to stdout, filtered by `RUST_LOG` (default `info`). Spans are
//! not exported anywhere yet.

use clap::ValueEnum;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// How a log line is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LogFormat {
    /// Human-readable lines.
    Text,
    /// One JSON object per line, for log collectors.
    Json,
}

/// Install the global subscriber. Fails if `RUST_LOG` is not a valid filter
/// or a subscriber is already installed.
pub fn init(format: LogFormat) -> anyhow::Result<()> {
    let filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::INFO.into())
        .from_env()?;
    let registry = tracing_subscriber::registry().with(filter);
    match format {
        LogFormat::Text => registry.with(tracing_subscriber::fmt::layer()).try_init()?,
        LogFormat::Json => registry
            .with(tracing_subscriber::fmt::layer().json())
            .try_init()?,
    }
    Ok(())
}
