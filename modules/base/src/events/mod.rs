//! Shared AMQP event contracts.
//!
//! `base` hosts the cross-module event payloads so that a publisher and a
//! consumer living in different feature crates never depend on each other's
//! internals — both depend only on `base`. Module-internal signals (cron ticks,
//! mail-send calls) stay in the owning module's `events`.
//!
//! Each event derives the `rkyv` traits plus `RkyvMessageSer`/`RkyvMessageDe`
//! and implements `AmqpRouting` + `AmqpMessageSend` from `wakuwaku::amqp`.

pub mod add_invitation_slot;
pub mod credit_change;
pub mod invitation_accepted;
pub mod invitation_sent;
pub mod system_ban;
pub mod user_login;
pub mod user_registered;

pub use add_invitation_slot::AddInvitationSlotEvent;
pub use credit_change::CreditChangeEvent;
pub use invitation_accepted::InvitationAcceptedEvent;
pub use invitation_sent::InvitationSentEvent;
pub use system_ban::SystemBanEvent;
pub use user_login::UserLoginEvent;
pub use user_registered::UserRegisteredEvent;
