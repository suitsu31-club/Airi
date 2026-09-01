use kanau::{RkyvMessageDe, RkyvMessageSer};
use uuid::Uuid;
use wakuwaku::amqp::{AmqpExchangeType, AmqpMessageSend, AmqpRouting};

/// **Public event**
///
/// Published by: `auth` (when a new account registers using someone's invite).
/// Consumed by: `messaging` (email the inviter that their invitation was
/// accepted, when they opted in via `send_invitation_email`).
/// Route: exchange `auth`, key `invitation_accepted`.
#[derive(
    Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, RkyvMessageSer, RkyvMessageDe,
)]
pub struct InvitationAcceptedEvent {
    /// The account that owns the consumed invite — i.e. who to notify.
    pub inviter_id: Uuid,
    /// The newly registered account admitted by the invite.
    pub new_member_id: Uuid,
    /// The new member's username, for the notification body.
    pub new_member_username: String,
    /// Acceptance timestamp (unix seconds).
    pub accepted_at: u64,
}

impl AmqpRouting for InvitationAcceptedEvent {
    const EXCHANGE: &'static str = "auth";
    const EXCHANGE_TYPE: AmqpExchangeType = AmqpExchangeType::Direct;
    const ROUTING_KEY: &'static str = "invitation_accepted";
}

impl AmqpMessageSend for InvitationAcceptedEvent {}
