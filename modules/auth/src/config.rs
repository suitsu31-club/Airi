//! Module configuration.
//!
//! Put the strongly typed configuration for this module here. The convention in
//! this stack is to store configuration as JSON in the database (one row per
//! key in a shared application-config table) and cache it in Redis so services
//! can load it cheaply and read-only at runtime. The management CLI seeds the
//! defaults; a refresh step copies the database value into the Redis cache.
//!
//! Define a `serde`-(de)serializable struct that implements `Default` and bind
//! it to a stable config key:
//!
//! ```ignore
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Debug, Clone, Serialize, Deserialize, Default)]
//! pub struct ExampleConfig {
//!     pub feature_enabled: bool,
//!     pub max_items: u32,
//! }
//!
//! // Bind the struct to the key used to store/lookup it in the database/Redis.
//! // The concrete `ConfigJson`-style trait is provided by whichever module in
//! // your workspace owns configuration storage.
//! //
//! // impl ConfigJson for ExampleConfig {
//! //     const KEY: &'static str = "example";
//! // }
//! ```
