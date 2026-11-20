//! # `base` — the foundational module
//!
//! `base` is the reference/shared module of this workspace. Every business
//! feature lives in its own `modules/<name>` crate that mirrors the layout of
//! this crate and depends on `base` for the types and helpers shared across the
//! whole application (common entities, error types, config primitives,
//! utilities, and so on).
//!
//! ## Module layout (the convention every module follows)
//!
//! - [`entities`] — persistence layer. Entity marker types plus query values
//!   (`wakuwaku::entity::{Query, Execute}`) that a capability-typed
//!   `wakuwaku::entity::Db` handle runs as a `Processor`. Split into
//!   [`entities::db`] (PostgreSQL rows and queries) and [`entities::redis`]
//!   (typed Redis collections and queries).
//! - [`services`] — business logic. Stateful `Processor`s that own their
//!   dependencies (`Db` handles, the Iggy `Publisher`, other services) and
//!   orchestrate entities to fulfil a use case.
//! - [`rpc`] — the transport edge. gRPC service implementations that translate
//!   protobuf requests into service/entity calls and back.
//! - [`events`] — Apache Iggy events this module publishes or consumes: the
//!   partition keys (topic) and the event types (type tag, ordering, algebra).
//! - [`hooks`] — background reactors: Iggy event handlers, cron jobs, and
//!   event loggers that run outside the request path.
//! - [`config`] — strongly typed configuration for the module, stored in the
//!   database and cached in Redis.
//! - [`utils`] — small, dependency-light helpers local to the module.
//!
//! `base` also owns [`error`]: the [`Error`] type services and hooks return,
//! which replaces the `wakuwaku::Error` removed in wakuwaku 1.0.
//!
//! See `AGENTS.md` at the workspace root for the full authoring guide.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(clippy::arithmetic_side_effects)]

pub mod config;
pub mod entities;
pub mod error;
pub mod events;
pub mod hooks;
pub mod rpc;
pub mod services;
pub mod utils;

pub use error::{Error, Result};