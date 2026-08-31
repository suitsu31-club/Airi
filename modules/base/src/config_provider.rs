//! Storage helpers for [`ConfigJson`] values.
//!
//! Configuration lives in the `base.application_config` Postgres table (source
//! of truth) and is mirrored into Redis at `config:{KEY}` for cheap runtime
//! reads. All readers fall back to [`Default`] on a missing or malformed value,
//! so configuration lookups never fail on absence.

use redis::AsyncCommands;
use wakuwaku::redis::RedisConnection;

use crate::config::ConfigJson;

/// Redis cache key for a configuration payload.
fn redis_key<T: ConfigJson>() -> String {
    format!("config:{}", T::KEY)
}

/// Load the configuration from the database, falling back to [`Default`] when
/// the row is missing or its JSON cannot be parsed into `T`.
#[tracing::instrument(skip_all, fields(config_key = T::KEY), err)]
pub async fn find_config_from_db<T: ConfigJson>(db: &sqlx::PgPool) -> Result<T, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT content FROM base.application_config WHERE key = $1"#,
        T::KEY
    )
    .fetch_optional(db)
    .await?;

    Ok(row
        .and_then(|r| serde_json::from_value(r.content).ok())
        .unwrap_or_default())
}

/// Load the configuration from the Redis cache, falling back to [`Default`] when
/// the key is missing or its bytes cannot be parsed into `T`.
#[tracing::instrument(skip_all, fields(config_key = T::KEY), err)]
pub async fn find_config_from_redis<T: ConfigJson>(
    conn: &mut RedisConnection,
) -> Result<T, wakuwaku::Error> {
    let data: Option<Vec<u8>> = conn.get(redis_key::<T>()).await?;
    Ok(data
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default())
}

/// Copy the database value into the Redis cache.
#[tracing::instrument(skip_all, fields(config_key = T::KEY), err)]
pub async fn refresh_config<T: ConfigJson>(
    db: &sqlx::PgPool,
    conn: &mut RedisConnection,
) -> Result<(), wakuwaku::Error> {
    let cfg = find_config_from_db::<T>(db).await?;
    let bytes = serde_json::to_vec(&cfg).map_err(|e| wakuwaku::Error::SerializeError(e.into()))?;
    let _: () = conn.set(redis_key::<T>(), bytes).await?;
    Ok(())
}

/// Insert the default configuration row if it does not already exist. Returns
/// `true` when a new row was inserted.
#[tracing::instrument(skip_all, fields(config_key = T::KEY), err)]
pub async fn insert_config_if_absent<T: ConfigJson>(
    db: &sqlx::PgPool,
    cfg: &T,
) -> Result<bool, wakuwaku::Error> {
    let value = serde_json::to_value(cfg).map_err(|e| wakuwaku::Error::SerializeError(e.into()))?;
    let result = sqlx::query!(
        r#"INSERT INTO base.application_config (key, content) VALUES ($1, $2)
           ON CONFLICT (key) DO NOTHING"#,
        T::KEY,
        value
    )
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Insert or replace the configuration row, bumping `last_updated_at`.
#[tracing::instrument(skip_all, fields(config_key = T::KEY), err)]
pub async fn upsert_config<T: ConfigJson>(
    db: &sqlx::PgPool,
    cfg: &T,
) -> Result<(), wakuwaku::Error> {
    let value = serde_json::to_value(cfg).map_err(|e| wakuwaku::Error::SerializeError(e.into()))?;
    sqlx::query!(
        r#"INSERT INTO base.application_config (key, content) VALUES ($1, $2)
           ON CONFLICT (key) DO UPDATE SET content = $2, last_updated_at = now()"#,
        T::KEY,
        value
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Read a raw configuration value by arbitrary key.
#[tracing::instrument(skip_all, err)]
pub async fn get_config_value(
    db: &sqlx::PgPool,
    key: &str,
) -> Result<Option<serde_json::Value>, sqlx::Error> {
    let row = sqlx::query!(
        r#"SELECT content FROM base.application_config WHERE key = $1"#,
        key
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|r| r.content))
}

/// Upsert a raw configuration value by arbitrary key and mirror it into the
/// Redis cache at `config:{key}`.
#[tracing::instrument(skip_all, err)]
pub async fn set_config_value(
    db: &sqlx::PgPool,
    conn: &mut RedisConnection,
    key: &str,
    value: &serde_json::Value,
) -> Result<(), wakuwaku::Error> {
    sqlx::query!(
        r#"INSERT INTO base.application_config (key, content) VALUES ($1, $2)
           ON CONFLICT (key) DO UPDATE SET content = $2, last_updated_at = now()"#,
        key,
        value
    )
    .execute(db)
    .await?;
    let bytes = serde_json::to_vec(value).map_err(|e| wakuwaku::Error::SerializeError(e.into()))?;
    let _: () = conn.set(format!("config:{key}"), bytes).await?;
    Ok(())
}
