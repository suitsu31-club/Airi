use kanau::{RkyvMessageDe, RkyvMessageSer};
use uuid::Uuid;
use wakuwaku::amqp::{AmqpExchangeType, AmqpMessageSend, AmqpRouting};

/// **Public event**
///
/// Published by: external systems that award a user invitation slots (loyalty,
/// promotions, partner integrations, …).
/// Consumed by: `auth` (mint that many `Free` invite slots for the user).
/// Route: exchange `invitation`, key `add_invitation_slot`.
///
/// This is the second way a user comes to hold invitation slots — the first
/// being an admin grant (`admin.GrantInvitations`). Users still cannot mint
/// slots themselves; only an admin or an external publisher of this event can.
#[derive(
    Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, RkyvMessageSer, RkyvMessageDe,
)]
pub struct AddInvitationSlotEvent {
    /// The account that should receive the invitation slots.
    pub user_id: Uuid,
    /// Number of `Free` slots to mint (must be positive; non-positive is rejected).
    pub count: i32,
    /// Optional lifetime for the granted slots, in seconds; `None` never expires.
    pub expire_in_secs: Option<u64>,
    /// Provenance recorded on each minted invite's `source` column.
    pub source: String,
}

impl AmqpRouting for AddInvitationSlotEvent {
    const EXCHANGE: &'static str = "invitation";
    const EXCHANGE_TYPE: AmqpExchangeType = AmqpExchangeType::Direct;
    const ROUTING_KEY: &'static str = "add_invitation_slot";
}

impl AmqpMessageSend for AddInvitationSlotEvent {}
