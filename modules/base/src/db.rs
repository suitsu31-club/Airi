//! Shared database connection helpers.

use sqlx::Executor;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Schemas made visible on every pooled connection so that schema-scoped custom
/// types (enums) resolve by their unqualified names.
const SEARCH_PATH: &str = "SET search_path TO auth, messaging, base, public";

/// Connect a Postgres pool with the workspace `search_path` applied to every
/// connection.
pub async fn connect_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                conn.execute(SEARCH_PATH).await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await
}
