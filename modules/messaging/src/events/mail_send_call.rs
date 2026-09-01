use kanau::{RkyvMessageDe, RkyvMessageSer};
use wakuwaku::amqp::{AmqpExchangeType, AmqpMessageSend, AmqpRouting};

/// **Internal sink event** — a request to send one email.
///
/// Published by: messaging email hooks. Consumed by: `MailerHook`.
/// Route: exchange `messaging`, key `mail_send`. `Debug` redacts the body.
#[derive(
    Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, RkyvMessageSer, RkyvMessageDe,
)]
pub struct MailSendCall {
    /// Optional override sender; defaults to the configured SMTP sender.
    pub from: Option<String>,
    /// Recipient email address.
    pub to: String,
    /// Email subject.
    pub subject: String,
    /// HTML body (secret-ish; redacted in `Debug`).
    pub body: String,
}

impl core::fmt::Debug for MailSendCall {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MailSendCall")
            .field("from", &self.from)
            .field("to", &self.to)
            .field("subject", &self.subject)
            .field("body", &"***")
            .finish()
    }
}

impl AmqpRouting for MailSendCall {
    const EXCHANGE: &'static str = "messaging";
    const EXCHANGE_TYPE: AmqpExchangeType = AmqpExchangeType::Direct;
    const ROUTING_KEY: &'static str = "mail_send";
}

impl AmqpMessageSend for MailSendCall {}
