//! Redis copy of `base.module_config`, read by
//! [`ConfigLoader`](crate::services::config::ConfigLoader).

use kanau::{RkyvMessageDe, RkyvMessageSer};
use std::time::Duration;
use wakuwaku::entity::redis::collection::cache::Cache;

/// A config value as its JSON text, named by
/// [`ModuleConfig::full_config_key`](crate::ModuleConfig::full_config_key).
///
/// Kept as JSON so one cache serves every config type, and so a value cached
/// before a config struct changed fails to decode instead of being misread.
#[derive(
    Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, RkyvMessageSer, RkyvMessageDe,
)]
pub struct CachedModuleConfig {
    pub json: String,
}

/// Updates remove the key, so the TTL only bounds how long a write made
/// outside [`ConfigUpdater`](crate::services::config::ConfigUpdater) (or one
/// racing a concurrent cache fill) stays invisible.
pub const MODULE_CONFIG_CACHE: Cache<str, CachedModuleConfig> =
    Cache::new("base_config", Duration::from_secs(300));
