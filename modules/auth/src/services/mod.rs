//! Business logic layer: stateful `Processor` services.
//!
//! Each service owns its dependencies (database, Redis, message queue, other
//! services) and implements one `Processor` per operation, returning domain
//! types. The [`crate::rpc`] edge adapts these to the wire format.

pub mod account;
pub mod admin;
pub mod api_key;
pub mod invitation;
pub mod login;
pub mod profile;
pub mod session;
