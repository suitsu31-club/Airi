//! Worker selection and shared dependency initialisation.

mod consumer;
mod cron;
mod grpc;
mod internal_grpc;
mod openapi;

use anyhow::Context;
use std::net::SocketAddr;
use wakuwaku::amqp::AmqpPool;
use wakuwaku::redis::RedisConnection;
use wakuwaku::sqlx::DatabaseProcessor;

/// The run mode of an app-server process.
#[derive(Debug, Clone, Copy)]
pub enum WorkMode {
    Grpc,
    InternalGrpc,
    Consumer,
    Cron,
    OpenApi,
}

/// Parsed launch configuration.
#[derive(Debug)]
pub struct WorkerArgs {
    pub mode: WorkMode,
    pub listen_addr: SocketAddr,
    pub database_url: String,
    pub redis_url: String,
    pub mq_url: String,
}

/// Shared runtime dependencies.
pub struct Deps {
    pub db: DatabaseProcessor,
    pub redis: RedisConnection,
    pub mq: AmqpPool,
}

impl WorkerArgs {
    /// Build worker configuration from environment variables.
    pub fn load_from_env() -> anyhow::Result<Self> {
        let mode = match std::env::var("WORK_MODE").unwrap_or_default().as_str() {
            "grpc" => WorkMode::Grpc,
            "internal_grpc" => WorkMode::InternalGrpc,
            "consumer" => WorkMode::Consumer,
            "cron" => WorkMode::Cron,
            "openapi" => WorkMode::OpenApi,
            other => anyhow::bail!(
                "invalid WORK_MODE {other:?}; expected grpc|internal_grpc|consumer|cron|openapi"
            ),
        };
        let listen_addr = std::env::var("LISTEN_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:50051".to_string())
            .parse()
            .context("invalid LISTEN_ADDR")?;
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL not set")?;
        let redis_url = std::env::var("REDIS_URL").context("REDIS_URL not set")?;
        let mq_url = std::env::var("MQ_URL").context("MQ_URL not set")?;
        Ok(Self {
            mode,
            listen_addr,
            database_url,
            redis_url,
            mq_url,
        })
    }

    /// Initialise dependencies and dispatch to the selected worker.
    pub async fn execute(self) -> anyhow::Result<()> {
        let deps = initialize_deps(&self.database_url, &self.redis_url, &self.mq_url).await?;
        match self.mode {
            WorkMode::Grpc => grpc::run(deps, self.listen_addr).await,
            WorkMode::InternalGrpc => internal_grpc::run(deps, self.listen_addr).await,
            WorkMode::Consumer => consumer::run(deps).await,
            WorkMode::Cron => cron::run(deps).await,
            WorkMode::OpenApi => openapi::run(deps, self.listen_addr).await,
        }
    }
}

async fn initialize_deps(
    database_url: &str,
    redis_url: &str,
    mq_url: &str,
) -> anyhow::Result<Deps> {
    let pool = base::db::connect_pool(database_url)
        .await
        .context("connecting to postgres")?;
    let db = DatabaseProcessor::from_pool(pool);

    let client = redis::Client::open(redis_url).context("opening redis client")?;
    let redis = client
        .get_multiplexed_async_connection()
        .await
        .context("connecting to redis")?;

    let args = amqprs::connection::OpenConnectionArguments::try_from(mq_url)
        .context("parsing MQ_URL")?;
    let conn = amqprs::connection::Connection::open(&args)
        .await
        .context("opening amqp connection")?;
    let mq = AmqpPool::connect(conn).await;

    Ok(Deps { db, redis, mq })
}
