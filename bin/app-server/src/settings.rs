//! Process settings, read from the command line or the environment.
//!
//! Only what the process needs to start lives here: where the shared
//! dependencies are and which workers to run. Settings an operator changes at
//! run time are module configuration in the database (`base::ModuleConfig`).

use crate::observability::LogFormat;
use crate::worker::Mode;
use clap::Parser;
use std::net::SocketAddr;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(version, about = "Runs the Airi modules behind the selected workers")]
pub struct Settings {
    /// Workers this process runs.
    #[arg(long, env = "APP_MODE", value_enum, default_value_t = Mode::All)]
    pub mode: Mode,

    /// PostgreSQL primary, used for every write.
    #[arg(long, env = "DATABASE_URL", hide_env_values = true)]
    pub database_url: String,

    /// PostgreSQL read replica for read-only handles. Without it they use the
    /// primary.
    #[arg(long, env = "DATABASE_READ_URL", hide_env_values = true)]
    pub database_read_url: Option<String>,

    /// Connections per PostgreSQL pool (primary and replica each).
    #[arg(long, env = "DATABASE_MAX_CONNECTIONS", default_value_t = 10)]
    pub database_max_connections: u32,

    /// Redis, e.g. `redis://127.0.0.1:6379`.
    #[arg(long, env = "REDIS_URL", hide_env_values = true)]
    pub redis_url: String,

    /// Iggy connection string with credentials, e.g.
    /// `iggy://iggy:iggy@127.0.0.1:8090`.
    #[arg(long, env = "IGGY_URL", hide_env_values = true)]
    pub iggy_url: String,

    /// Iggy stream holding every topic of the application.
    #[arg(long, env = "IGGY_STREAM", default_value = "airi")]
    pub iggy_stream: String,

    /// Index of this consumer replica, in `0..CONSUMER_SHARDS`.
    #[arg(long, env = "CONSUMER_SHARD", default_value_t = 0)]
    pub consumer_shard: u32,

    /// Number of consumer replicas. Replica `i` consumes the partitions whose
    /// ID modulo this count is `i`, so every partition has one consumer.
    #[arg(long, env = "CONSUMER_SHARDS", default_value_t = 1)]
    pub consumer_shards: u32,

    /// Address of the gRPC listener (modes `grpc` and `all`).
    #[arg(long, env = "GRPC_LISTEN", default_value = "0.0.0.0:50051")]
    pub grpc_listen: SocketAddr,

    /// Address of the HTTP liveness and readiness probes (every mode).
    #[arg(long, env = "HEALTH_LISTEN", default_value = "0.0.0.0:8080")]
    pub health_listen: SocketAddr,

    /// Seconds to let in-flight requests, event batches and cron runs finish
    /// after a shutdown signal before they are cancelled.
    #[arg(long, env = "SHUTDOWN_TIMEOUT_SECS", default_value_t = 30)]
    pub shutdown_timeout_secs: u64,

    /// Log line format. The filter comes from `RUST_LOG` (default `info`).
    #[arg(long, env = "LOG_FORMAT", value_enum, default_value_t = LogFormat::Text)]
    pub log_format: LogFormat,
}

impl Settings {
    pub const fn shutdown_timeout(&self) -> Duration {
        Duration::from_secs(self.shutdown_timeout_secs)
    }
}
