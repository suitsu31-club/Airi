//! Module configuration primitives.
//!
//! Strongly typed module configuration is stored as JSON in the shared
//! `base.application_config` table (one row per [`ConfigJson::KEY`]) and cached
//! in Redis at `config:{KEY}` so services can load it cheaply at runtime. The
//! management CLI seeds the defaults; [`crate::config_provider`] provides the
//! read/refresh/seed helpers.
//!
//! Each module defines a `serde`-(de)serializable struct implementing `Default`
//! and binds it to a stable key:
//!
//! ```ignore
//! use base::config::ConfigJson;
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Clone, Serialize, Deserialize, Default)]
//! pub struct ExampleConfig {
//!     pub feature_enabled: bool,
//!     pub max_items: u32,
//! }
//!
//! impl ConfigJson for ExampleConfig {
//!     const KEY: &'static str = "example";
//! }
//! ```

use serde::Serialize;
use serde::de::DeserializeOwned;

/// A typed configuration payload bound to a stable storage key.
///
/// Implementors are stored as JSON in `base.application_config` and cached in
/// Redis. A missing or malformed value always falls back to [`Default`], so a
/// service can load configuration without ever failing on absence.
pub trait ConfigJson: Default + Serialize + DeserializeOwned + Send + Sync {
    /// Stable key used to store and look up this configuration.
    const KEY: &'static str;
}
