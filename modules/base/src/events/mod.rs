//! AMQP event definitions.
//!
//! Put the message payloads this module publishes or consumes here. Events are
//! how modules communicate asynchronously without depending on each other's
//! internals: one module publishes, others react from their [`hooks`](crate::hooks).
//!
//! Document each event's contract in a doc comment — who publishes it, who
//! consumes it, and its routing key. Derive the `rkyv` traits for the payload
//! and implement `AmqpRouting` + `AmqpMessageSend` from `wakuwaku::amqp`.
//!
//! ```ignore
//! use kanau::{RkyvMessageDe, RkyvMessageSer};
//! use wakuwaku::amqp::{AmqpExchangeType, AmqpMessageSend, AmqpRouting};
//!
//! /// **Public event**
//! ///
//! /// Published by: base module
//! /// Consumed by: (your consumers)
//! /// Route: exchange `base`, key `example_happened`
//! #[derive(
//!     Debug, Clone,
//!     rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
//!     RkyvMessageSer, RkyvMessageDe,
//! )]
//! pub struct ExampleHappened {
//!     pub id: uuid::Uuid,
//! }
//!
//! impl AmqpRouting for ExampleHappened {
//!     const EXCHANGE: &'static str = "base";
//!     const EXCHANGE_TYPE: AmqpExchangeType = AmqpExchangeType::Direct;
//!     const ROUTING_KEY: &'static str = "example_happened";
//! }
//!
//! impl AmqpMessageSend for ExampleHappened {}
//! ```
