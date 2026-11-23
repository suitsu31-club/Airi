//! Apache Iggy event definitions.
//!
//! Put the events this module publishes or consumes here. Events are how
//! modules communicate asynchronously without depending on each other's
//! internals: one module publishes with `wakuwaku::iggy::Publisher`, others
//! react from their [`hooks`](crate::hooks).
//!
//! An event is described by two traits from `wakuwaku::iggy`:
//!
//! - [`PartitionKey`](wakuwaku::iggy::PartitionKey) on the key type — the thing
//!   whose events must stay in order (a user, an order). Its
//!   `SUPER_PARTITION_NAME` is the Iggy **topic**; the key hash picks the
//!   partition. Events that must be ordered against each other share one key
//!   type.
//! - [`Event`](wakuwaku::iggy::Event) on the payload — its `TYPE_TAG` (unique
//!   within the topic, never renumbered or reused), its key, its
//!   `atomic_ordering` relative to the key's other event types, and its
//!   `Algebra` (`NonAssociative` when in doubt).
//!
//! The payload is also an [`EventBody`](wakuwaku::iggy::EventBody): derive the
//! `rkyv` traits plus `RkyvMessageSer` / `RkyvMessageDe`. The Iggy stream is
//! deployment configuration (chosen in `app-server`), not part of the event.
//!
//! Document each event's contract in a doc comment — who publishes it, who
//! consumes it, its topic and type tag.
//!
//! ```ignore
//! use std::hash::DefaultHasher;
//!
//! use kanau::{RkyvMessageDe, RkyvMessageSer};
//! use wakuwaku::iggy::{
//!     Event, EventAtomicOrdering, EventBody, EventTypeTag, IggySuperPartitionName,
//!     NonAssociative, PartitionKey,
//! };
//!
//! /// Every event keyed by an example goes to the `base_example` topic.
//! /// `DefaultHasher` is stable while every publisher uses the pinned toolchain.
//! #[derive(Hash)]
//! pub struct ExampleKey(pub uuid::Uuid);
//!
//! impl PartitionKey for ExampleKey {
//!     type PartitionMerge = DefaultHasher;
//!     const SUPER_PARTITION_NAME: IggySuperPartitionName = IggySuperPartitionName("base_example");
//! }
//!
//! /// **Public event**
//! ///
//! /// Published by: base module
//! /// Consumed by: (your consumers)
//! /// Route: topic `base_example`, type tag `1`, keyed by [`ExampleKey`]
//! #[derive(
//!     Debug, Clone,
//!     rkyv::Archive, rkyv::Serialize, rkyv::Deserialize,
//!     RkyvMessageSer, RkyvMessageDe,
//! )]
//! pub struct ExampleHappened {
//!     pub id: uuid::Uuid,
//! }
//!
//! impl EventBody for ExampleHappened {}
//!
//! impl Event for ExampleHappened {
//!     const TYPE_TAG: EventTypeTag = EventTypeTag::new(1);
//!     type Key = ExampleKey;
//!     type Algebra = NonAssociative;
//!
//!     fn key(&self) -> ExampleKey {
//!         ExampleKey(self.id)
//!     }
//!
//!     fn atomic_ordering(&self) -> EventAtomicOrdering {
//!         EventAtomicOrdering::Relaxed
//!     }
//! }
//! ```
//!
//! wakuwaku does not create the stream or the main topic: create both (with
//! their partition count) before publishing or consuming; see `manage-tool`.
