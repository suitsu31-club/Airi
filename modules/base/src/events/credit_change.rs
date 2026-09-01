use kanau::{RkyvMessageDe, RkyvMessageSer};
use uuid::Uuid;
use wakuwaku::amqp::{AmqpExchangeType, AmqpMessageSend, AmqpRouting};

/// **Public event**
///
/// Published by: external credit/billing systems.
/// Consumed by: `auth` (apply the delta to `auth.credit` and append history).
/// Route: exchange `credit`, key `credit_change`.
///
/// The deltas are transported as decimal strings (e.g. `"12.50"`, `"-3"`)
/// because `rust_decimal` does not implement the workspace's `rkyv` version.
/// Consumers parse them with `rust_decimal::Decimal::from_str`.
#[derive(
    Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, RkyvMessageSer, RkyvMessageDe,
)]
pub struct CreditChangeEvent {
    /// The account whose credit changes.
    pub user_id: Uuid,
    /// Change to the available balance as a decimal string (may be negative).
    pub available_delta: String,
    /// Change to the frozen balance as a decimal string (may be negative).
    pub frozen_delta: String,
    /// Human-readable reason recorded in the change history.
    pub reason: String,
}

impl AmqpRouting for CreditChangeEvent {
    const EXCHANGE: &'static str = "credit";
    const EXCHANGE_TYPE: AmqpExchangeType = AmqpExchangeType::Direct;
    const ROUTING_KEY: &'static str = "credit_change";
}

impl AmqpMessageSend for CreditChangeEvent {}
