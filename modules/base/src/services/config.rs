//! Loading and updating [`ModuleConfig`] values.
//!
//! Reads go through Redis and fall back to PostgreSQL on a miss; an update
//! writes PostgreSQL and removes the Redis key, so every replica reloads it.

use crate::ModuleConfig;
use crate::entities::db::config::{FindModuleConfig, UpsertModuleConfig};
use crate::entities::redis::config::{CachedModuleConfig, MODULE_CONFIG_CACHE};
use kanau::message::SerializeError;
use kanau::processor::Processor;
use std::marker::PhantomData;
use wakuwaku::entity::redis::RedisSource;
use wakuwaku::entity::sqlx::backend::PgSource;
use wakuwaku::entity::{Db, ReadOnly, ReadWrite};

/// Load the current value of `T`: `T::default()` while no value is stored.
pub struct LoadConfig<T: ModuleConfig>(PhantomData<fn() -> T>);

impl<T: ModuleConfig> LoadConfig<T> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: ModuleConfig> Default for LoadConfig<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Store `value` as the current value of `T`.
pub struct UpdateConfig<T: ModuleConfig> {
    pub value: T,
}

/// Reads configuration. PostgreSQL is only read on a cache miss, so it may be
/// a replica.
#[derive(Clone)]
pub struct ConfigLoader {
    pg: Db<PgSource, ReadOnly>,
    redis: Db<RedisSource, ReadWrite>,
}

impl ConfigLoader {
    pub const fn new(pg: Db<PgSource, ReadOnly>, redis: Db<RedisSource, ReadWrite>) -> Self {
        Self { pg, redis }
    }
}

impl<T: ModuleConfig> Processor<LoadConfig<T>> for ConfigLoader {
    type Output = T;
    type Error = crate::Error;

    #[tracing::instrument(skip_all, err, fields(key))]
    async fn process(&self, _: LoadConfig<T>) -> Result<T, Self::Error> {
        let key = T::full_config_key();
        tracing::Span::current().record("key", key.as_str());

        if let Some(cached) = self.redis.process(MODULE_CONFIG_CACHE.get(&key)).await? {
            match serde_json::from_str(&cached.json) {
                Ok(value) => return Ok(value),
                // Cached by a build with another shape of `T`: reload it.
                Err(error) => tracing::warn!(%error, "discarding an undecodable cached config"),
            }
        }

        let value = self.pg.process(FindModuleConfig::<T>::new()).await?;
        let json = serde_json::to_string(&value).map_err(SerializeError::from)?;
        self.redis
            .process(MODULE_CONFIG_CACHE.put(&key, CachedModuleConfig { json }))
            .await?;
        Ok(value)
    }
}

/// Writes configuration.
#[derive(Clone)]
pub struct ConfigUpdater {
    pg: Db<PgSource, ReadWrite>,
    redis: Db<RedisSource, ReadWrite>,
}

impl ConfigUpdater {
    pub const fn new(pg: Db<PgSource, ReadWrite>, redis: Db<RedisSource, ReadWrite>) -> Self {
        Self { pg, redis }
    }
}

impl<T: ModuleConfig> Processor<UpdateConfig<T>> for ConfigUpdater {
    type Output = ();
    type Error = crate::Error;

    #[tracing::instrument(skip_all, err, fields(key))]
    async fn process(&self, input: UpdateConfig<T>) -> Result<(), Self::Error> {
        let key = T::full_config_key();
        tracing::Span::current().record("key", key.as_str());

        self.pg
            .process(UpsertModuleConfig { value: input.value })
            .await?;
        self.redis.process(MODULE_CONFIG_CACHE.remove(&key)).await?;
        Ok(())
    }
}
