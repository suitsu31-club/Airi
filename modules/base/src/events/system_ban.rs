use kanau::{RkyvMessageDe, RkyvMessageSer};
use uuid::Uuid;
use wakuwaku::amqp::{AmqpExchangeType, AmqpMessageSend, AmqpRouting};

/// **Public event**
///
/// Published by: external moderation systems.
/// Consumed by: `auth` (record suspense status; terminate sessions on ban).
/// Route: exchange `moderation`, key `system_ban`.
#[derive(
    Debug,
    Clone,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    RkyvMessageSer,
    RkyvMessageDe,
)]
pub struct SystemBanEvent {
    /// The account being banned or unbanned.
    pub user_id: Uuid,
    /// `true` to ban (suspend), `false` to unban (reactivate).
    pub banned: bool,
    /// Reason recorded on the suspense entry.
    pub reason: String,
    /// The operator responsible, if any.
    pub operated_by: Option<Uuid>,
}

impl AmqpRouting for SystemBanEvent {
    const EXCHANGE: &'static str = "moderation";
    const EXCHANGE_TYPE: AmqpExchangeType = AmqpExchangeType::Direct;
    const ROUTING_KEY: &'static str = "system_ban";
}

impl AmqpMessageSend for SystemBanEvent {}
