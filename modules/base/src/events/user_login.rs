use kanau::{RkyvMessageDe, RkyvMessageSer};
use uuid::Uuid;
use wakuwaku::amqp::{AmqpExchangeType, AmqpMessageSend, AmqpRouting};

/// **Public event**
///
/// Published by: `auth` (on successful login).
/// Consumed by: `messaging` (send a login-notification email when enabled).
/// Route: exchange `auth`, key `user_login`.
#[derive(
    Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, RkyvMessageSer, RkyvMessageDe,
)]
pub struct UserLoginEvent {
    /// The account that logged in.
    pub user_id: Uuid,
    /// The IP the login originated from.
    pub ip: String,
    /// The user agent of the login client.
    pub user_agent: String,
    /// Login timestamp (unix seconds).
    pub at: u64,
}

impl AmqpRouting for UserLoginEvent {
    const EXCHANGE: &'static str = "auth";
    const EXCHANGE_TYPE: AmqpExchangeType = AmqpExchangeType::Direct;
    const ROUTING_KEY: &'static str = "user_login";
}

impl AmqpMessageSend for UserLoginEvent {}
