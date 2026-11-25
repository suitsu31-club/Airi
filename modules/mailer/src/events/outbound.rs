//! Topic `mailer_outbound`: messages waiting for a transport.

use kanau::{RkyvMessageDe, RkyvMessageSer};
use std::hash::DefaultHasher;
use wakuwaku::iggy::{
    Event, EventAtomicOrdering, EventBody, EventTypeTag, IggySuperPartitionName, NonAssociative,
    PartitionKey,
};

/// Routes events of one stored message (by its mail ID) to `mailer_outbound`.
/// `DefaultHasher` is stable while every publisher uses the pinned toolchain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OutboundKey(pub i64);

impl PartitionKey for OutboundKey {
    type PartitionMerge = DefaultHasher;
    const SUPER_PARTITION_NAME: IggySuperPartitionName = IggySuperPartitionName("mailer_outbound");
}

/// **Internal event**: a message was stored and should be handed to the
/// transport.
///
/// Published by: `mailer` (`SendEmail`, and the operator's "send now" of a
/// held message)
/// Consumed by: `mailer` transport hooks (the SMTP relay handler)
/// Route: topic `mailer_outbound`, type tag `1`, keyed by [`OutboundKey`]
///
/// Carries only the mail ID: recipients and bodies (which may contain sign-in
/// links) stay in PostgreSQL. Delivery is at least once, so the handler skips
/// a message that is no longer queued.
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
pub struct EmailQueued {
    pub mail_id: i64,
}

impl EventBody for EmailQueued {}

impl Event for EmailQueued {
    const TYPE_TAG: EventTypeTag = EventTypeTag::new(1);
    type Key = OutboundKey;
    type Algebra = NonAssociative;

    fn key(&self) -> OutboundKey {
        OutboundKey(self.mail_id)
    }

    fn atomic_ordering(&self) -> EventAtomicOrdering {
        EventAtomicOrdering::Relaxed
    }
}
