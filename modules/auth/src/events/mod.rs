//! Module-internal AMQP signals.
//!
//! Cross-module event contracts live in [`base::events`]; this module only
//! defines the periodic cron signals the `auth` cleanup hooks consume.

use kanau::{RkyvMessageDe, RkyvMessageSer};
use std::task::Poll;
use time::OffsetDateTime;
use wakuwaku::amqp::{AmqpExchangeType, AmqpMessageSend, AmqpRouting};
use wakuwaku::interval_job::IntervalJobExecutionSignal;

/// Cadence between cron ticks, in seconds.
const CRON_CADENCE_SECS: i64 = 3_600;

/// **Internal signal** — triggers expired-session cleanup.
///
/// Published by: the `cron` worker. Consumed by: `AuthCronHook`.
/// Route: exchange `auth_cron`, key `session_cleanup`.
#[derive(
    Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, RkyvMessageSer, RkyvMessageDe,
)]
pub struct SessionCleanupSignal {
    /// Tick time (unix seconds).
    pub trigger_time: u64,
}

impl AmqpRouting for SessionCleanupSignal {
    const EXCHANGE: &'static str = "auth_cron";
    const EXCHANGE_TYPE: AmqpExchangeType = AmqpExchangeType::Direct;
    const ROUTING_KEY: &'static str = "session_cleanup";
}

impl AmqpMessageSend for SessionCleanupSignal {}

impl IntervalJobExecutionSignal for SessionCleanupSignal {
    fn tick(now: OffsetDateTime) -> Self {
        Self {
            trigger_time: now.unix_timestamp().max(0) as u64,
        }
    }
    fn time_pool(now: OffsetDateTime, last_time: OffsetDateTime) -> Poll<Self> {
        if (now - last_time).whole_seconds() >= CRON_CADENCE_SECS {
            Poll::Ready(Self::tick(now))
        } else {
            Poll::Pending
        }
    }
}

/// **Internal signal** — triggers invitation/pending-invitation expiry cleanup.
///
/// Published by: the `cron` worker. Consumed by: `AuthCronHook`.
/// Route: exchange `auth_cron`, key `invitation_expiry`.
#[derive(
    Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, RkyvMessageSer, RkyvMessageDe,
)]
pub struct InvitationExpiryCleanupSignal {
    /// Tick time (unix seconds).
    pub trigger_time: u64,
}

impl AmqpRouting for InvitationExpiryCleanupSignal {
    const EXCHANGE: &'static str = "auth_cron";
    const EXCHANGE_TYPE: AmqpExchangeType = AmqpExchangeType::Direct;
    const ROUTING_KEY: &'static str = "invitation_expiry";
}

impl AmqpMessageSend for InvitationExpiryCleanupSignal {}

impl IntervalJobExecutionSignal for InvitationExpiryCleanupSignal {
    fn tick(now: OffsetDateTime) -> Self {
        Self {
            trigger_time: now.unix_timestamp().max(0) as u64,
        }
    }
    fn time_pool(now: OffsetDateTime, last_time: OffsetDateTime) -> Poll<Self> {
        if (now - last_time).whole_seconds() >= CRON_CADENCE_SECS {
            Poll::Ready(Self::tick(now))
        } else {
            Poll::Pending
        }
    }
}
