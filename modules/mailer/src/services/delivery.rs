//! `MailDeliveryService`: hands queued messages to their sender's SMTP relay
//! (feature `delivery`), and sends test messages for operators.
//!
//! Each message is sent over its own connection, so a sender's decrypted
//! credentials live only while that message is sent.
//!
//! Delivery is at least once. A message is sent only while it is `queued`,
//! and marked `sent` right after the relay accepts it; if the process stops
//! in between, the redelivered `EmailQueued` finds the message still queued
//! and sends it again.

use crate::config::{MailerConfig, MailerMode};
use crate::entities::db::send_box::{
    CancelMail, EmailFormat, FindMailById, MailId, MailStatus, MarkMailAttempted, MarkMailFailed,
    MarkMailSent, SendBoxEntity,
};
use crate::entities::db::sender::{
    FindSmtpSenderById, SmtpSenderEntity, SmtpSenderId, SmtpTlsMode,
};
use crate::events::delivery::EmailSent;
use anyhow::anyhow;
use base::Error;
use base::services::config::{ConfigLoader, LoadConfig};
use itertools::Itertools;
use kanau::processor::Processor;
use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Address, AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use std::sync::Arc;
use time::{OffsetDateTime, PrimitiveDateTime};
use type_safe_data::encryption::Key256;
use type_safe_data::mail::EmailAddress;
use wakuwaku::entity::sqlx::backend::PgSource;
use wakuwaku::entity::{Db, ReadWrite};
use wakuwaku::iggy::Publisher;

/// Delivers queued messages, or only marks them sent in mocked mode.
#[derive(Clone)]
pub struct MailDeliveryService {
    db: Db<PgSource, ReadWrite>,
    publisher: Publisher,
    config: ConfigLoader,
    /// `None` without `MAILER_SECRET_KEY`: only senders without credentials
    /// can deliver.
    secret_key: Option<Arc<Key256>>,
}

impl MailDeliveryService {
    /// `secret_key` is the key from
    /// [`secret_key_from_env`](crate::utils::secret_key::secret_key_from_env).
    pub const fn new(
        db: Db<PgSource, ReadWrite>,
        publisher: Publisher,
        config: ConfigLoader,
        secret_key: Option<Arc<Key256>>,
    ) -> Self {
        Self {
            db,
            publisher,
            config,
            secret_key,
        }
    }
}

/// Deliver one stored message.
#[derive(Debug, Clone, Copy)]
pub struct DeliverMail {
    pub mail_id: MailId,
}

/// What [`DeliverMail`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// The relay accepted it, or mocked mode marked it sent without
    /// delivering. Also the outcome for a message that was already sent:
    /// `EmailSent` is published again, in case the run that sent it stopped
    /// before publishing.
    Sent,
    /// The relay rejected it permanently (5xx), or it can never be sent;
    /// the reason is stored.
    Failed,
    /// Its `send_before_or_cancel` passed.
    Cancelled,
    /// It is not waiting for delivery: held, failed, cancelled, unknown, or
    /// changed by a concurrent run.
    Skipped,
}

impl Processor<DeliverMail> for MailDeliveryService {
    type Output = Delivery;
    type Error = Error;

    /// Connection failures and 4xx replies fail with a retryable error and
    /// leave the message queued.
    #[tracing::instrument(skip_all, err, fields(mail_id = input.mail_id.0))]
    async fn process(&self, input: DeliverMail) -> Result<Delivery, Error> {
        let Some(mail) = self
            .db
            .process(FindMailById {
                mail_id: input.mail_id,
            })
            .await?
        else {
            tracing::warn!("no message has this ID");
            return Ok(Delivery::Skipped);
        };
        match mail.status {
            MailStatus::Queued => {}
            MailStatus::Sent => {
                if let Some(sent_at) = mail.sent_at {
                    self.publish_sent(mail.mail_id, sent_at).await?;
                }
                return Ok(Delivery::Sent);
            }
            MailStatus::Held | MailStatus::Failed | MailStatus::Cancelled => {
                return Ok(Delivery::Skipped);
            }
        }

        if mail
            .send_before_or_cancel
            .is_some_and(|deadline| deadline <= now_utc())
        {
            let cancelled = self
                .db
                .process(CancelMail {
                    mail_id: mail.mail_id,
                })
                .await?;
            return Ok(if cancelled {
                Delivery::Cancelled
            } else {
                Delivery::Skipped
            });
        }

        let config = self
            .config
            .process(LoadConfig::<MailerConfig>::new())
            .await?;
        // Mocked mode delivers nothing. A queued message always has a sender
        // (the table stores sender-less messages as sent).
        let (MailerMode::Smtp, Some(sender_id)) = (config.mode, mail.send_as) else {
            return self.mark_sent(mail.mail_id).await;
        };

        let sender = self
            .db
            .process(FindSmtpSenderById { id: sender_id })
            .await?
            .ok_or(Error::NotFound)?;
        if !sender.enabled {
            return self.fail(mail.mail_id, "the sender is disabled").await;
        }
        let message = match build_message(&sender, &mail) {
            Ok(message) => message,
            Err(reason) => return self.fail(mail.mail_id, reason).await,
        };
        let transport = self.transport(&sender)?;

        if !self
            .db
            .process(MarkMailAttempted {
                mail_id: mail.mail_id,
            })
            .await?
        {
            return Ok(Delivery::Skipped);
        }
        match transport.send(message).await {
            Ok(_) => self.mark_sent(mail.mail_id).await,
            Err(error) if error.is_permanent() => self.fail(mail.mail_id, error.to_string()).await,
            Err(error) => Err(Error::Io(error.into())),
        }
    }
}

/// Send one test message to `send_to` through a sender, so an operator can
/// check its relay settings and credentials. Works whatever the mode and
/// whether the sender is enabled; nothing is stored.
#[derive(Debug, Clone)]
pub struct TestTransport {
    pub sender_id: SmtpSenderId,
    pub send_to: EmailAddress,
}

/// What the relay answered to [`TestTransport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayAnswer {
    /// The relay accepted the message.
    pub accepted: bool,
    /// The relay's reply (`250 …`), or why the message was not accepted:
    /// its rejection, or the connection or TLS failure.
    pub reply: String,
}

impl Processor<TestTransport> for MailDeliveryService {
    type Output = RelayAnswer;
    type Error = Error;

    /// Fails with `NotFound` for an unknown sender, and with `Io` when the
    /// credentials can't be decrypted; the relay's answer is the output.
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: TestTransport) -> Result<RelayAnswer, Error> {
        let sender = self
            .db
            .process(FindSmtpSenderById {
                id: input.sender_id,
            })
            .await?
            .ok_or(Error::NotFound)?;
        let message = test_message(&sender, &input.send_to)
            .map_err(|reason| Error::BusinessPanic(anyhow!(reason)))?;
        Ok(match self.transport(&sender)?.send(message).await {
            Ok(response) => RelayAnswer {
                accepted: true,
                reply: format!("{} {}", response.code(), response.message().join(" ")),
            },
            Err(error) => RelayAnswer {
                accepted: false,
                reply: error.to_string(),
            },
        })
    }
}

impl MailDeliveryService {
    /// A transport for one message through `sender`, holding its decrypted
    /// credentials.
    fn transport(
        &self,
        sender: &SmtpSenderEntity,
    ) -> Result<AsyncSmtpTransport<Tokio1Executor>, Error> {
        let port = u16::try_from(sender.port).map_err(|_| {
            Error::BusinessPanic(anyhow!("SMTP port {} is out of range", sender.port))
        })?;
        let builder = match sender.tls_mode {
            SmtpTlsMode::StartTls => {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&sender.host)
            }
            SmtpTlsMode::ImplicitTls => AsyncSmtpTransport::<Tokio1Executor>::relay(&sender.host),
            SmtpTlsMode::Plaintext => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
                sender.host.as_str(),
            )),
        }
        .map_err(|error| Error::Io(error.into()))?
        .port(port);
        Ok(match self.credentials(sender)? {
            Some(credentials) => builder.credentials(credentials),
            None => builder,
        }
        .build())
    }

    /// The sender's decrypted credentials; `None` for a relay without
    /// authentication. A missing or wrong `MAILER_SECRET_KEY` is retryable:
    /// the operator fixes it without touching the message.
    fn credentials(&self, sender: &SmtpSenderEntity) -> Result<Option<Credentials>, Error> {
        let (Some(username), Some(password)) =
            (&sender.username_encrypted, &sender.password_encrypted)
        else {
            return Ok(None);
        };
        let key = self.secret_key.as_deref().ok_or_else(|| {
            Error::Io(anyhow!(
                "MAILER_SECRET_KEY is unset; it is needed to decrypt the sender's credentials"
            ))
        })?;
        let username = username
            .decrypt(key, &sender.id.username_associated_data())
            .map_err(|error| Error::Io(error.into()))?;
        let password = password
            .decrypt(key, &sender.id.password_associated_data())
            .map_err(|error| Error::Io(error.into()))?;
        Ok(Some(Credentials::new(
            String::clone(&username),
            String::clone(&password),
        )))
    }

    /// `queued` → `sent`, then report it.
    async fn mark_sent(&self, mail_id: MailId) -> Result<Delivery, Error> {
        let Some(sent_at) = self.db.process(MarkMailSent { mail_id }).await? else {
            return Ok(Delivery::Skipped);
        };
        self.publish_sent(mail_id, sent_at).await?;
        Ok(Delivery::Sent)
    }

    async fn publish_sent(&self, mail_id: MailId, sent_at: PrimitiveDateTime) -> Result<(), Error> {
        let sent_at_unix_ms = sent_at
            .assume_utc()
            .unix_timestamp_nanos()
            .div_euclid(1_000_000);
        self.publisher
            .publish(EmailSent {
                mail_id: mail_id.0,
                sent_at_unix_ms: i64::try_from(sent_at_unix_ms).unwrap_or(i64::MAX),
            })
            .await?;
        Ok(())
    }

    /// `queued` → `failed`, keeping `reason`.
    async fn fail(&self, mail_id: MailId, reason: impl Into<String>) -> Result<Delivery, Error> {
        let failed = self
            .db
            .process(MarkMailFailed {
                mail_id,
                failure_reply: reason.into(),
            })
            .await?;
        Ok(if failed {
            Delivery::Failed
        } else {
            Delivery::Skipped
        })
    }
}

/// The message as sent, or why it can never be sent.
fn build_message(sender: &SmtpSenderEntity, mail: &SendBoxEntity) -> Result<Message, String> {
    let content = mail
        .message_content
        .as_ref()
        .ok_or("the message body was deleted")?;
    let to = Mailbox::new(
        Some(mail.send_to_name.clone()).filter(|name| !name.is_empty()),
        parse_address(mail.send_to_address.as_ref())?,
    );
    let content_type = match mail.format {
        EmailFormat::Plain => ContentType::TEXT_PLAIN,
        EmailFormat::Html => ContentType::TEXT_HTML,
    };
    compose(sender, to, &mail.title, content_type, content.0.clone())
}

/// The message [`TestTransport`] sends.
fn test_message(sender: &SmtpSenderEntity, send_to: &EmailAddress) -> Result<Message, String> {
    compose(
        sender,
        Mailbox::new(None, parse_address(send_to.as_ref())?),
        &format!("Test message from sender {}", sender.name),
        ContentType::TEXT_PLAIN,
        format!(
            "The sender {} delivered this message through {}:{}.\n",
            sender.name, sender.host, sender.port
        ),
    )
}

fn compose(
    sender: &SmtpSenderEntity,
    to: Mailbox,
    subject: &str,
    content_type: ContentType,
    body: String,
) -> Result<Message, String> {
    // `message_id(None)` generates `<random@this host>`.
    Message::builder()
        .message_id(None)
        .from(Mailbox::new(None, parse_address(sender.address.as_ref())?))
        .to(to)
        .subject(subject)
        .header(content_type)
        .body(body)
        .map_err(|error| error.to_string())
}

fn parse_address(address: &str) -> Result<Address, String> {
    address
        .parse()
        .map_err(|_| format!("invalid address {address}"))
}

fn now_utc() -> PrimitiveDateTime {
    let now = OffsetDateTime::now_utc();
    PrimitiveDateTime::new(now.date(), now.time())
}
