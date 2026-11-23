//! Redis entities: cached and ephemeral state.
//!
//! Put your Redis-backed types here — one submodule per key kind (sessions,
//! one-time tokens, rate-limit counters, and so on).
//!
//! Redis is a data source like PostgreSQL (`wakuwaku::entity::redis`): a
//! `Db<RedisSource, ReadOnly | ReadWrite>` handle runs typed queries. Prefer
//! the ready-made collections — `Cache` / `HashedCache` (values with a TTL),
//! `Lock` / `RwLock`, `TokenBucket` — declared as `const` values whose methods
//! build queries. Each collection gets its own namespace, prefixed with the
//! module name. Bodies implement kanau's `MessageSer` / `MessageDe`; derive the
//! `rkyv` traits plus `RkyvMessageSer` / `RkyvMessageDe`. Key types implement
//! `wakuwaku::entity::redis::Name` (strings and integers already do).
//!
//! ```ignore
//! use std::io::Write;
//! use std::time::Duration;
//!
//! use kanau::{RkyvMessageDe, RkyvMessageSer};
//! use wakuwaku::entity::redis::Name;
//! use wakuwaku::entity::redis::collection::cache::Cache;
//!
//! #[derive(Debug, Clone, Copy)]
//! pub struct ExampleId(pub uuid::Uuid);
//!
//! impl Name for ExampleId {
//!     fn write_name(&self, key: &mut Vec<u8>) {
//!         // Writing to a `Vec` never fails.
//!         let _ = write!(key, "{}", self.0);
//!     }
//! }
//!
//! #[derive(
//!     Debug, Clone,
//!     rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
//!     RkyvMessageSer, RkyvMessageDe,
//! )]
//! pub struct ExampleCache {
//!     pub value: u64,
//! }
//!
//! /// `redis.process(EXAMPLE_CACHE.get(&id))`, `.put(&id, body)`, `.remove(&id)`.
//! pub const EXAMPLE_CACHE: Cache<ExampleId, ExampleCache> =
//!     Cache::new("base.example", Duration::from_secs(300));
//! ```
//!
//! For a command the collections don't cover, write a query yourself:
//! [`Query`](wakuwaku::entity::Query) plus
//! `Execute<RedisSource<C>>` for `C: redis::aio::ConnectionLike + Clone + Send + Sync`,
//! failing with `redis::RedisError`.
