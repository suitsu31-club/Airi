//! The shared dependencies, connected once at startup.
//!
//! Schema migrations and Iggy topics are not touched here: `manage-tool`
//! applies and provisions them before the server starts.

use crate::settings::Settings;
use anyhow::Context;
use redis::aio::MultiplexedConnection;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use wakuwaku::deps::iggy::prelude::{Client, IggyClient};
use wakuwaku::iggy::Publisher;

/// Connections shared by every module. Cloning shares them.
#[derive(Clone)]
pub struct Dependencies {
    /// The PostgreSQL primary.
    pub pg: PgPool,
    /// The PostgreSQL read replica, or a clone of [`pg`](Self::pg) without
    /// one.
    pub pg_read: PgPool,
    /// Whether [`pg_read`](Self::pg_read) is a separate replica.
    pub has_read_replica: bool,
    pub redis: MultiplexedConnection,
    pub iggy: Arc<IggyClient>,
    /// Publishes to [`Settings::iggy_stream`].
    pub publisher: Publisher,
}

impl Dependencies {
    pub async fn connect(settings: &Settings) -> anyhow::Result<Self> {
        let max_connections = settings.database_max_connections;
        let pg = connect_pg(&settings.database_url, max_connections)
            .await
            .context("connecting to the PostgreSQL primary (DATABASE_URL)")?;
        let (pg_read, has_read_replica) = match &settings.database_read_url {
            Some(url) => (
                connect_pg(url, max_connections)
                    .await
                    .context("connecting to the PostgreSQL replica (DATABASE_READ_URL)")?,
                true,
            ),
            None => (pg.clone(), false),
        };

        let redis = redis::Client::open(settings.redis_url.as_str())
            .context("parsing REDIS_URL")?
            .get_multiplexed_async_connection()
            .await
            .context("connecting to Redis (REDIS_URL)")?;

        let iggy = Arc::new(
            IggyClient::from_connection_string(&settings.iggy_url).context("parsing IGGY_URL")?,
        );
        iggy.connect().await.context("connecting to Iggy (IGGY_URL)")?;
        let publisher =
            Publisher::new(iggy.clone(), &settings.iggy_stream).context("parsing IGGY_STREAM")?;

        Ok(Self {
            pg,
            pg_read,
            has_read_replica,
            redis,
            iggy,
            publisher,
        })
    }
}

async fn connect_pg(url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(url)
        .await
}
