//! Redis entities: cached and ephemeral key/value state.
//!
//! Put your Redis-backed types here — one submodule per key kind (sessions,
//! one-time tokens, rate-limit counters, and so on).
//!
//! The convention: define a value type and its key type, derive the `rkyv`
//! traits for zero-copy (de)serialization, and implement the `KeyValue` family
//! (`KeyValue`, `KeyValueRead`, `KeyValueWrite`) from `wakuwaku::redis` so reads
//! and writes are type-safe.
//!
//! ```ignore
//! use kanau::{RkyvMessageDe, RkyvMessageSer};
//! use wakuwaku::redis::{KeyValue, KeyValueRead, KeyValueWrite};
//!
//! #[derive(
//!     Debug, Clone,
//!     rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
//!     RkyvMessageSer, RkyvMessageDe,
//! )]
//! pub struct ExampleCache {
//!     pub id: ExampleId,
//!     pub value: u64,
//! }
//!
//! #[derive(Debug, Clone, Copy, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
//! pub struct ExampleId(pub uuid::Uuid);
//!
//! impl KeyValue for ExampleCache {
//!     type Key = ExampleId;
//!     type Value = Self;
//!     fn key(&self) -> Self::Key { self.id }
//!     fn value(&self) -> Self::Value { self.clone() }
//!     fn into_value(self) -> Self::Value { self }
//!     fn new(key: Self::Key, mut value: Self::Value) -> Self {
//!         value.id = key;
//!         value
//!     }
//! }
//!
//! impl KeyValueRead for ExampleCache {}
//! impl KeyValueWrite for ExampleCache {}
//! ```
