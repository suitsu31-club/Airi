use kanau::{RkyvMessageDe, RkyvMessageSer};
use wakuwaku::amqp::{AmqpExchangeType, AmqpMessageSend, AmqpRouting};

/// **Public event**
///
/// Published by: `auth` (when an invitation email is (re)sent).
/// Consumed by: `messaging` (render and dispatch the invitation email).
/// Route: exchange `auth`, key `invitation_sent`.
///
/// The `invite_token` is a secret; its `Debug` representation is redacted.
#[derive(
    Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, RkyvMessageSer, RkyvMessageDe,
)]
pub struct InvitationSentEvent {
    /// The pending-invitation id.
    pub invite_id: i64,
    /// The recipient email address.
    pub email: String,
    /// The opaque invite token (secret — redacted in `Debug`).
    pub invite_token: String,
    /// Send timestamp (unix seconds).
    pub sent_at: u64,
}

impl core::fmt::Debug for InvitationSentEvent {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InvitationSentEvent")
            .field("invite_id", &self.invite_id)
            .field("email", &self.email)
            .field("invite_token", &"***")
            .field("sent_at", &self.sent_at)
            .finish()
    }
}

impl AmqpRouting for InvitationSentEvent {
    const EXCHANGE: &'static str = "auth";
    const EXCHANGE_TYPE: AmqpExchangeType = AmqpExchangeType::Direct;
    const ROUTING_KEY: &'static str = "invitation_sent";
}

impl AmqpMessageSend for InvitationSentEvent {}
