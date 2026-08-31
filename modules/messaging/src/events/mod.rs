//! Messaging-internal AMQP events.
//!
//! Cross-module contracts live in [`base::events`]; this module owns only the
//! internal mail-send sink event.

pub mod mail_send_call;

pub use mail_send_call::MailSendCall;
