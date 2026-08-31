use kanau::{RkyvMessageDe, RkyvMessageSer};
use uuid::Uuid;
use wakuwaku::amqp::{AmqpExchangeType, AmqpMessageSend, AmqpRouting};

/// **Public event**
///
/// Published by: `auth` (on successful registration).
/// Consumed by: `messaging` (initialise per-user notification settings).
/// Route: exchange `auth`, key `user_registered`.
#[derive(
    Debug,
    Clone,
    rkyv::Archive,
    rkyv::Serialize,
    rkyv::Deserialize,
    RkyvMessageSer,
    RkyvMessageDe,
)]
pub struct UserRegisteredEvent {
    /// The newly created account id.
    pub user_id: Uuid,
    /// The account's email address.
    pub email: String,
    /// The invite id that admitted this user, if registration was invite-based.
    pub invited_by: Option<i64>,
    /// Registration timestamp (unix seconds).
    pub registered_at: u64,
}

impl AmqpRouting for UserRegisteredEvent {
    const EXCHANGE: &'static str = "auth";
    const EXCHANGE_TYPE: AmqpExchangeType = AmqpExchangeType::Direct;
    const ROUTING_KEY: &'static str = "user_registered";
}

impl AmqpMessageSend for UserRegisteredEvent {}
