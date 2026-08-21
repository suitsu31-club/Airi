//! Background reactors that run outside the request path.
//!
//! Hooks are where a module reacts to things instead of being called directly:
//!
//! - **AMQP consumers** — implement `AmqpMessageProcessor<E>` plus
//!   `Processor<E>` to handle an [`event`](crate::events) delivered from the
//!   queue.
//! - **Cron jobs** — periodic tasks (cleanup, aggregation, reconciliation).
//! - **Event loggers** — persist or forward events for audit/observability.
//!
//! Like services, a hook is a `Clone`-able struct that owns its dependencies.
//! The `app-server` binary wires hooks into its consumer/cron workers.
//!
//! ```ignore
//! use kanau::processor::Processor;
//! use wakuwaku::amqp::AmqpMessageProcessor;
//! use wakuwaku::redis::RedisConnection;
//!
//! use crate::events::ExampleHappened;
//!
//! #[derive(Clone)]
//! pub struct ExampleHook {
//!     pub redis: RedisConnection,
//! }
//!
//! // Bind the hook to a durable queue name.
//! impl AmqpMessageProcessor<ExampleHappened> for ExampleHook {
//!     const QUEUE: &'static str = "app_base_example";
//! }
//!
//! impl Processor<ExampleHappened> for ExampleHook {
//!     type Output = ();
//!     type Error = wakuwaku::Error;
//!     async fn process(&self, input: ExampleHappened) -> Result<(), Self::Error> {
//!         // react to the event...
//!         Ok(())
//!     }
//! }
//! ```
