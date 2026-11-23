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
