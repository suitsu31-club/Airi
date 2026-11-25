//! `SendEmail`: how other modules send a message.
//!
//! [`MailerService`] stores the message and hands it to the transport by
//! publishing [`EmailQueued`]; it never talks to a relay itself. The caller
//! first proves its sender is [`ActiveSender`]:
//!
//! ```ignore
//! use wakuwaku::proof::{CheckOrder, ValidateError, prove};
//!
//! let send_as = match prove::<_, ActiveSender>(name, &mailer, CheckOrder::Sequential).await {
//!     Ok(sender) => Some(sender),
//!     Err(ValidateError::Deny(reason)) => return Err(/* unknown or disabled: `reason` */),
//!     Err(ValidateError::IoError(error)) => return Err(error),
//! };
//! mailer.process(SendEmail { send_as, ..mail }).await?;
//! ```

use crate::config::{MailerConfig, MailerMode};
use crate::entities::db::send_box::{InsertMail, InsertMailOutcome};
use crate::entities::db::sender::{FindSmtpSenderByName, SmtpSenderId};
use crate::events::outbound::EmailQueued;
use base::Error;
use base::services::config::{ConfigLoader, LoadConfig};
use kanau::processor::Processor;
use time::PrimitiveDateTime;
use type_safe_data::mail::EmailAddress;
use wakuwaku::entity::sqlx::backend::PgSource;
use wakuwaku::entity::{Db, ReadWrite};
use wakuwaku::iggy::Publisher;
use wakuwaku::proof::{
    CheckOrder, DeniedReason, Predicate, Proven, ValidateContext, ValidateError,
};

pub use crate::entities::db::send_box::{EmailFormat, MailContent, MailId, MailStatus};

/// No sender has the name.
pub static SENDER_NOT_FOUND: DeniedReason = DeniedReason("no sender has this name");
/// The sender exists but is disabled.
pub static SENDER_DISABLED: DeniedReason = DeniedReason("the sender is disabled");

/// Stores messages and queues them for the transport.
#[derive(Clone)]
pub struct MailerService {
    db: Db<PgSource, ReadWrite>,
    publisher: Publisher,
    config: ConfigLoader,
}

impl MailerService {
    pub const fn new(
        db: Db<PgSource, ReadWrite>,
        publisher: Publisher,
        config: ConfigLoader,
    ) -> Self {
        Self {
            db,
            publisher,
            config,
        }
    }
}

impl ValidateContext for MailerService {
    type IoError = Error;
}

/// The sender named by the subject exists and is enabled. Denied with
/// [`SENDER_NOT_FOUND`] or [`SENDER_DISABLED`].
#[derive(Debug)]
pub struct ActiveSender {
    id: SmtpSenderId,
    address: EmailAddress,
}

impl ActiveSender {
    /// The sender's From address.
    pub fn address(&self) -> &EmailAddress {
        &self.address
    }
}

impl Predicate<String> for ActiveSender {
    type Ctx = MailerService;

    #[tracing::instrument(skip_all, err)]
    async fn check(
        name: &String,
        ctx: &MailerService,
        _: CheckOrder,
    ) -> Result<Self, ValidateError<Error>> {
        let sender = ctx
            .db
            .process(FindSmtpSenderByName { name: name.clone() })
            .await
            .map_err(|error| ValidateError::IoError(error.into()))?;
        match sender {
            None => Err(ValidateError::Deny(SENDER_NOT_FOUND)),
            Some(sender) if !sender.enabled => Err(ValidateError::Deny(SENDER_DISABLED)),
            Some(sender) => Ok(Self {
                id: sender.id,
                address: sender.address,
            }),
        }
    }
}

/// Send one message to one recipient.
///
/// A repeated request (same `requested_at`, sender, recipient and message)
/// returns the message stored the first time and stores nothing.
#[derive(Debug)]
pub struct SendEmail {
    /// When the caller decided to send this message (UTC), e.g. the time of
    /// the event it reacts to. Must be the same on every retry of the
    /// request: never the time `SendEmail` runs.
    pub requested_at: PrimitiveDateTime,
    /// `None`: the message is stored as sent and never delivered.
    pub send_as: Option<Proven<String, ActiveSender>>,
    pub send_to_address: EmailAddress,
    /// Display name of the recipient, empty for none.
    pub send_to_name: String,
    pub title: String,
    pub content: MailContent,
    pub format: EmailFormat,
    /// UTC. A message still unsent at this time is cancelled instead.
    pub send_before_or_cancel: Option<PrimitiveDateTime>,
}

/// The stored message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptedMail {
    pub mail_id: MailId,
    /// `Queued`: handed to the transport. `Held`: kept for an operator
    /// (mocked mode). `Sent`: there was no sender. Any other status: the
    /// request was repeated after the message was processed.
    pub status: MailStatus,
}

impl Processor<SendEmail> for MailerService {
    type Output = AcceptedMail;
    type Error = Error;

    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: SendEmail) -> Result<AcceptedMail, Error> {
        let config = self
            .config
            .process(LoadConfig::<MailerConfig>::new())
            .await?;
        let status = match config.mode {
            MailerMode::Mocked => MailStatus::Held,
            MailerMode::Smtp => MailStatus::Queued,
        };
        let outcome = self
            .db
            .process(InsertMail {
                requested_at: input.requested_at,
                send_to_address: input.send_to_address,
                send_to_name: input.send_to_name,
                send_as: input.send_as.map(|sender| sender.evidence().id),
                title: input.title,
                message_content: input.content,
                format: input.format,
                status,
                send_before_or_cancel: input.send_before_or_cancel,
            })
            .await?;
        // A repeated request publishes again while the message is still
        // queued: the first attempt may have failed between storing and
        // publishing. The transport skips a message that is not queued.
        let (InsertMailOutcome::Created(mail) | InsertMailOutcome::Existing(mail)) = outcome;
        if mail.status == MailStatus::Queued {
            self.publisher
                .publish(EmailQueued {
                    mail_id: mail.mail_id.0,
                })
                .await?;
        }
        Ok(AcceptedMail {
            mail_id: mail.mail_id,
            status: mail.status,
        })
    }
}
