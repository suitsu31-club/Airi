//! Newtypes that carry an invariant (validated, encrypted, …) through every
//! layer, so the invariant is checked once at the edge.

#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![warn(clippy::arithmetic_side_effects)]

pub mod encryption;
pub mod mail;
