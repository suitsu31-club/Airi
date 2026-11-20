//! Persistence layer: data types and the queries that read/write them.
//!
//! Entities are split by backing store:
//!
//! - [`db`] — PostgreSQL rows (`sqlx::FromRow` structs) and the queries that
//!   operate on them.
//! - [`redis`] — Redis collections and queries used for caching and ephemeral
//!   state.
//!
//! Each query or command is a small input struct implementing
//! `wakuwaku::entity::Query` (what it reads/writes) and `Execute<Source>` (how
//! it runs). A `Db<Source, Capability>` handle runs it as a `Processor`, so
//! persistence logic stays testable and composable, and the compiler rejects
//! writes through a read-only handle.

pub mod db;
pub mod redis;
