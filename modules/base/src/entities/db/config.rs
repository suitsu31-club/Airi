//! The `base.module_config` table: one JSON value per [`ModuleConfig`] key.

use sqlx::PgConnection;
use sqlx::types::Json;
use std::marker::PhantomData;
use time::PrimitiveDateTime;
use wakuwaku::entity::sqlx::backend::PgSource;
use wakuwaku::entity::{Entity, Execute, Query, Read, Write};

/// A row of `base.module_config`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ModuleConfigEntity {
    pub key: String,
    pub value: Json<serde_json::Value>,
    /// UTC.
    pub last_updated_at: PrimitiveDateTime,
}

impl Entity for ModuleConfigEntity {}

/// A typed setting an operator can change, stored as JSON under
/// [`full_config_key`](Self::full_config_key).
///
/// [`Default`] is the value in effect while no row exists.
pub trait ModuleConfig:
    serde::Serialize + for<'de> serde::Deserialize<'de> + Default + Send + Sync + Unpin + 'static
{
    /// The owning module, e.g. `@base`.
    const CONFIG_PREFIX: &'static str;
    /// The setting's name inside the module, e.g. `site_metadata`.
    const CONFIG_KEY: &'static str;

    fn full_config_key() -> String {
        format!("{}/{}", Self::CONFIG_PREFIX, Self::CONFIG_KEY)
    }
}

/// Read the stored value of `T`, or `T::default()` when no row exists.
pub struct FindModuleConfig<T: ModuleConfig>(PhantomData<fn() -> T>);

impl<T: ModuleConfig> FindModuleConfig<T> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T: ModuleConfig> Default for FindModuleConfig<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ModuleConfig> Query for FindModuleConfig<T> {
    type Effect = Read<ModuleConfigEntity>;
}

impl<T: ModuleConfig> Execute<PgSource> for FindModuleConfig<T> {
    type Output = T;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        let key = T::full_config_key();
        let stored = sqlx::query_scalar!(
            r#"SELECT "value" AS "value: Json<T>" FROM "base"."module_config" WHERE "key" = $1"#,
            key
        )
        .fetch_optional(conn)
        .await?;
        Ok(stored.map(|Json(value)| value).unwrap_or_default())
    }
}

/// Insert or replace the stored value of `T`.
pub struct UpsertModuleConfig<T: ModuleConfig> {
    pub value: T,
}

impl<T: ModuleConfig> Query for UpsertModuleConfig<T> {
    type Effect = Write<ModuleConfigEntity>;
}

impl<T: ModuleConfig> Execute<PgSource> for UpsertModuleConfig<T> {
    type Output = ();
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        let key = T::full_config_key();
        sqlx::query!(
            r#"
            INSERT INTO "base"."module_config" ("key", "value")
            VALUES ($1, $2)
            ON CONFLICT ("key") DO UPDATE
            SET "value" = EXCLUDED."value",
                "last_updated_at" = now() AT TIME ZONE 'UTC'
            "#,
            key,
            Json(&self.value) as _
        )
        .execute(conn)
        .await?;
        Ok(())
    }
}
