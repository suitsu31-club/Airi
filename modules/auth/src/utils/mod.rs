//! Small, self-contained helpers shared across this module.
//!
//! Keep this for pure, dependency-light utilities: formatting, encoding,
//! validation helpers, ID generation, and the like. Anything that owns runtime
//! dependencies (database, Redis, message queue) belongs in
//! [`services`](crate::services) instead.

pub mod config;
pub mod datetime;
pub mod identity;
pub mod password;
pub mod rbac;
pub mod totp;
