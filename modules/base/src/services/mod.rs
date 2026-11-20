//! Business logic layer.
//!
//! A service is a stateful `Processor`: a `Clone`-able struct that owns its
//! dependencies (`Db` handles for PostgreSQL / Redis, the Iggy `Publisher`,
//! other services, loaded config) and implements one `Processor` per operation
//! it supports, failing with [`crate::Error`].
//!
//! Services orchestrate [`entities`](crate::entities), publish
//! [`events`](crate::events), and are the layer the [`rpc`](crate::rpc) edge
//! calls into. Keep transport concerns (protobuf, HTTP) out of here — a service
//! should be usable regardless of how it is invoked.
//!
//! Take the narrowest capability a service needs: `Db<PgSource, ReadOnly>`
//! for one that only reads (it can then run on a replica), `ReadWrite` for one
//! that writes. Implement `wakuwaku::ServiceCreation` when the service is built
//! from a single registered dependency; otherwise `app-server` builds it with
//! `ServiceBuilder::compose_subservice`.
//!
//! ```ignore
//! use kanau::processor::Processor;
//! use wakuwaku::entity::sqlx::backend::PgSource;
//! use wakuwaku::entity::{Db, ReadWrite};
//! use wakuwaku::iggy::Publisher;
//!
//! use crate::entities::db::FindExampleById;
//! use crate::events::ExampleHappened;
//!
//! #[derive(Clone)]
//! pub struct ExampleService {
//!     pub db: Db<PgSource, ReadWrite>,
//!     pub publisher: Publisher,
//! }
//!
//! pub struct DoSomething {
//!     pub id: uuid::Uuid,
//! }
//!
//! impl Processor<DoSomething> for ExampleService {
//!     type Output = ();
//!     type Error = crate::Error;
//!
//!     #[tracing::instrument(skip_all, err)]
//!     async fn process(&self, input: DoSomething) -> Result<(), Self::Error> {
//!         let row = self
//!             .db
//!             .process(FindExampleById { id: input.id })
//!             .await?
//!             .ok_or(crate::Error::NotFound)?;
//!         // apply business rules, persist...
//!         self.publisher.publish(ExampleHappened { id: row.id }).await?;
//!         Ok(())
//!     }
//! }
//! ```
