//! Topic `mailer_delivery`: outcomes of handing messages to a transport.
//!
//! Kept apart from `mailer_outbound` so the transport consumer never reads its
//! own reports, and other modules can follow delivery without consuming the
//! outbound queue.

use kanau::{RkyvMessageDe, RkyvMessageSer};
use std::hash::DefaultHasher;
use wakuwaku::iggy::{
    Event, EventAtomicOrdering, EventBody, EventTypeTag, IggySuperPartitionName, NonAssociative,
    PartitionKey,
};

/// Routes delivery reports of one stored message (by its mail ID) to
/// `mailer_delivery`. `DefaultHasher` is stable while every publisher uses the
/// pinned toolchain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeliveryKey(pub i64);

impl PartitionKey for DeliveryKey {
    type PartitionMerge = DefaultHasher;
    const SUPER_PARTITION_NAME: IggySuperPartitionName = IggySuperPartitionName("mailer_delivery");
}

/// **Public event**: the relay accepted a message.
///
/// Published by: `mailer` transport hooks, after the message is marked sent
/// Consumed by: modules that track delivery (e.g. `notification`)
/// Route: topic `mailer_delivery`, type tag `1`, keyed by [`DeliveryKey`]
///
/// `mail_id` is the ID `SendEmail` returned. Delivery is at least once, so a
/// consumer may see the same message reported more than once.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    RkyvMessageSer,
    RkyvMessageDe,
)]
pub struct EmailSent {
    pub mail_id: i64,
    /// When the relay accepted the message, in Unix milliseconds (UTC).
    pub sent_at_unix_ms: i64,
}

impl EventBody for EmailSent {}

impl Event for EmailSent {
    const TYPE_TAG: EventTypeTag = EventTypeTag::new(1);
    type Key = DeliveryKey;
    type Algebra = NonAssociative;

    fn key(&self) -> DeliveryKey {
        DeliveryKey(self.mail_id)
    }

    fn atomic_ordering(&self) -> EventAtomicOrdering {
        EventAtomicOrdering::Relaxed
    }
}
