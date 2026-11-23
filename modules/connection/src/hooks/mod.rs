//! Background reactors that run outside the request path.
//!
//! Hooks are where a module reacts to things instead of being called directly:
//!
//! - **Iggy event handlers** — implement `wakuwaku::iggy::EventHandler<E>` to
//!   handle an [`event`](crate::events). A handler receives a *run*: up to
//!   `MAX_BATCH` events of one key, in order, already reduced by the event's
//!   algebra. Delivery is at least once, so handlers must be idempotent.
//! - **Cron jobs** — implement `Processor<CronTick>` plus `wakuwaku::CronJob`
//!   (with its `INTERVAL`). Every replica runs its own jobs, so make them
//!   idempotent or take a Redis `Lock`.
//! - **Event loggers** — persist or forward events for audit/observability.
//!
//! Like services, a hook is a struct that owns its dependencies and implements
//! `wakuwaku::ServiceCreation`, so `app-server` can build it from its
//! `ServiceBuilder` (`.iggy_consumer::<Hook, _, _>()`, `.cron_job::<Job, _>()`).
//! A hook that needs several dependencies skips `ServiceCreation` and is built
//! with `.compose_iggy_consumer(..)` / `.compose_cron_job(..)` instead.
//!
//! Failures are `HandleError`s; their class decides what the consumer does
//! (`transient`: retry in place, `retryable`: move the key to the retry topic,
//! `unrecoverable`: drop and log). [`crate::Error`] converts into one, so `?`
//! works on service calls.
//!
//! ```ignore
//! use std::num::NonZeroUsize;
//! use std::time::Duration;
//!
//! use kanau::processor::Processor;
//! use wakuwaku::entity::redis::RedisSource;
//! use wakuwaku::entity::{Db, ReadWrite};
//! use wakuwaku::iggy::EventHandler;
//! use wakuwaku::iggy::error::HandleError;
//! use wakuwaku::{CronJob, CronJobInterval, CronTick, ServiceCreation};
//!
//! use crate::entities::redis::{EXAMPLE_CACHE, ExampleId};
//! use crate::events::ExampleHappened;
//!
//! /// Drops the cached example whenever it changes.
//! pub struct ExampleHook {
//!     redis: Db<RedisSource, ReadWrite>,
//! }
//!
//! impl ServiceCreation for ExampleHook {
//!     type Dep = Db<RedisSource, ReadWrite>;
//!     fn create_service(redis: Self::Dep) -> Self {
//!         Self { redis }
//!     }
//! }
//!
//! impl EventHandler<ExampleHappened> for ExampleHook {
//!     const MAX_BATCH: NonZeroUsize = NonZeroUsize::MIN;
//!
//!     #[tracing::instrument(skip_all, err)]
//!     async fn handle(&self, run: &[ExampleHappened]) -> Result<(), HandleError> {
//!         for event in run {
//!             self.redis
//!                 .process(EXAMPLE_CACHE.remove(&ExampleId(event.id)))
//!                 .await
//!                 .map_err(crate::Error::from)?;
//!         }
//!         Ok(())
//!     }
//! }
//!
//! /// A periodic job.
//! pub struct ExampleCleanup {
//!     redis: Db<RedisSource, ReadWrite>,
//! }
//!
//! impl ServiceCreation for ExampleCleanup {
//!     type Dep = Db<RedisSource, ReadWrite>;
//!     fn create_service(redis: Self::Dep) -> Self {
//!         Self { redis }
//!     }
//! }
//!
//! impl Processor<CronTick> for ExampleCleanup {
//!     type Output = ();
//!     type Error = crate::Error;
//!
//!     #[tracing::instrument(skip_all, err)]
//!     async fn process(&self, tick: CronTick) -> Result<(), Self::Error> {
//!         // Read "now" from `tick.fired_at`, so tests control time.
//!         Ok(())
//!     }
//! }
//!
//! impl CronJob for ExampleCleanup {
//!     const INTERVAL: CronJobInterval = CronJobInterval::AtLeast(Duration::from_secs(60));
//! }
//! ```
