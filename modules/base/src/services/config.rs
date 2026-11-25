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

#[cfg(test)]
mod tests {
    //! Needs PostgreSQL (`DATABASE_URL`; each test gets a fresh database) and
    //! Redis (`REDIS_URL`, default `redis://127.0.0.1:6379`). Every test uses
    //! its own config key, so tests don't share cache entries.

    use super::*;
    use serde::{Deserialize, Serialize};
    use sqlx::PgPool;

    macro_rules! test_config {
        ($name:ident, $key:literal) => {
            #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
            struct $name {
                greeting: String,
            }

            impl $name {
                fn new(greeting: &str) -> Self {
                    Self {
                        greeting: greeting.to_owned(),
                    }
                }
            }

            impl Default for $name {
                fn default() -> Self {
                    Self::new("default")
                }
            }

            impl ModuleConfig for $name {
                const CONFIG_PREFIX: &'static str = "@base_test";
                const CONFIG_KEY: &'static str = $key;
            }
        };
    }

    test_config!(MissingConfig, "missing");
    test_config!(RoundTripConfig, "round_trip");
    test_config!(InvalidatedConfig, "invalidated");
    test_config!(StaleShapeConfig, "stale_shape");

    /// A loader and an updater on `pool`, with `T`'s cache entry cleared.
    async fn services<T: ModuleConfig>(pool: PgPool) -> (ConfigLoader, ConfigUpdater) {
        let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".into());
        let connection = redis::Client::open(url)
            .unwrap()
            .get_multiplexed_async_connection()
            .await
            .unwrap();
        let redis: Db<RedisSource, ReadWrite> = Db::new(RedisSource::new(connection));
        redis
            .process(MODULE_CONFIG_CACHE.remove(&T::full_config_key()))
            .await
            .unwrap();
        let source = PgSource::new(pool);
        (
            ConfigLoader::new(Db::new(source.clone()), redis.clone()),
            ConfigUpdater::new(Db::new(source), redis),
        )
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn missing_row_loads_the_default(pool: PgPool) {
        let (loader, _) = services::<MissingConfig>(pool).await;
        let loaded = loader
            .process(LoadConfig::<MissingConfig>::new())
            .await
            .unwrap();
        assert_eq!(loaded, MissingConfig::default());
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn updated_value_round_trips(pool: PgPool) {
        let (loader, updater) = services::<RoundTripConfig>(pool).await;
        let value = RoundTripConfig::new("hello");
        updater
            .process(UpdateConfig {
                value: value.clone(),
            })
            .await
            .unwrap();

        // The first load fills the cache from PostgreSQL, the second reads it.
        for _ in 0..2 {
            let loaded = loader
                .process(LoadConfig::<RoundTripConfig>::new())
                .await
                .unwrap();
            assert_eq!(loaded, value);
        }
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn update_invalidates_the_cached_value(pool: PgPool) {
        let (loader, updater) = services::<InvalidatedConfig>(pool).await;
        let before = loader
            .process(LoadConfig::<InvalidatedConfig>::new())
            .await
            .unwrap();
        assert_eq!(before, InvalidatedConfig::default());

        let value = InvalidatedConfig::new("changed");
        updater
            .process(UpdateConfig {
                value: value.clone(),
            })
            .await
            .unwrap();

        let after = loader
            .process(LoadConfig::<InvalidatedConfig>::new())
            .await
            .unwrap();
        assert_eq!(after, value);
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn undecodable_cache_entry_is_reloaded(pool: PgPool) {
        let (loader, updater) = services::<StaleShapeConfig>(pool).await;
        let value = StaleShapeConfig::new("stored");
        updater
            .process(UpdateConfig {
                value: value.clone(),
            })
            .await
            .unwrap();
        loader
            .redis
            .process(MODULE_CONFIG_CACHE.put(
                &StaleShapeConfig::full_config_key(),
                CachedModuleConfig {
                    json: r#"{"renamed_field":1}"#.to_owned(),
                },
            ))
            .await
            .unwrap();

        let loaded = loader
            .process(LoadConfig::<StaleShapeConfig>::new())
            .await
            .unwrap();
        assert_eq!(loaded, value);
    }
}
