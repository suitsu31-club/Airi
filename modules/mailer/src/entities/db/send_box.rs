//! The `mailer.send_box` table: every message `SendEmail` accepted, and how
//! far its delivery got.
//!
//! `SendEmail` stores a message `queued`, for the transport to deliver. Each
//! status change below is guarded by the status it leaves, so a repeated
//! event or a race between two replicas changes a message at most once.

use crate::entities::db::sender::SmtpSenderId;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::postgres::{PgArgumentBuffer, PgTypeInfo, PgValueRef};
use sqlx::{Decode, Encode, PgConnection, Postgres, Type};
use std::fmt::{self, Debug, Formatter};
use time::PrimitiveDateTime;
use type_safe_data::mail::EmailAddress;
use wakuwaku::entity::sqlx::backend::PgSource;
use wakuwaku::entity::{Entity, Execute, Query, Read, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type)]
#[sqlx(transparent)]
pub struct MailId(pub i64);

/// The body of a message. Its `Debug` shows only the length: bodies carry
/// sign-in links and must not reach the logs.
#[derive(Clone, PartialEq, Eq, sqlx::Type)]
#[sqlx(transparent)]
pub struct MailContent(pub String);

impl Debug for MailContent {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("MailContent")
            .field("len", &self.0.len())
            .finish()
    }
}

/// Identifies one request to send a message, so that a repeated request
/// stores and sends nothing. Derived by [`InsertMail::idempotency_key`];
/// stored as 32 bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, sqlx::Type)]
#[sqlx(transparent)]
pub struct IdempotencyKey([u8; 32]);

impl Debug for IdempotencyKey {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("IdempotencyKey(")?;
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        f.write_str(")")
    }
}

/// A row of `mailer.send_box`.
#[derive(Debug, Clone)]
pub struct SendBoxEntity {
    pub mail_id: MailId,
    /// Unique: storing a message under a key that is already used returns
    /// the stored message instead.
    pub idempotency_key: IdempotencyKey,
    pub send_to_address: EmailAddress,
    /// Display name of the recipient, empty for none.
    pub send_to_name: String,
    /// `None`: there is no sender to deliver through, so the message was
    /// marked sent when it was stored and is never delivered.
    pub send_as: Option<SmtpSenderId>,

    pub title: String,
    /// `None` once the retention job deleted it.
    pub message_content: Option<MailContent>,
    pub format: EmailFormat,

    pub status: MailStatus,
    /// What the relay answered; set exactly when `status` is `Failed`.
    pub failure_reply: Option<String>,

    /// UTC.
    pub created_at: PrimitiveDateTime,
    /// UTC. When the message last became `Queued`; always set while it is.
    pub queued_at: Option<PrimitiveDateTime>,
    /// UTC. When the transport last tried to deliver it; `None` if it never
    /// did.
    pub attempted_at: Option<PrimitiveDateTime>,
    /// UTC; set exactly when `status` is `Sent`.
    pub sent_at: Option<PrimitiveDateTime>,
    /// UTC. The mail must be sent before this time, or it will be cancelled.
    pub send_before_or_cancel: Option<PrimitiveDateTime>,
}

impl Entity for SendBoxEntity {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailFormat {
    Plain,
    Html,
}

impl EmailFormat {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Html => "html",
        }
    }
}

impl Type<Postgres> for EmailFormat {
    fn type_info() -> PgTypeInfo {
        <&str as Type<Postgres>>::type_info()
    }

    fn compatible(ty: &PgTypeInfo) -> bool {
        <&str as Type<Postgres>>::compatible(ty)
    }
}

impl Encode<'_, Postgres> for EmailFormat {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<IsNull, BoxDynError> {
        <&str as Encode<Postgres>>::encode(self.as_str(), buf)
    }
}

impl<'r> Decode<'r, Postgres> for EmailFormat {
    fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
        match <&str as Decode<Postgres>>::decode(value)? {
            "plain" => Ok(Self::Plain),
            "html" => Ok(Self::Html),
            other => Err(format!("unknown email format {other:?}").into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailStatus {
    /// Waiting for the transport.
    Queued,
    /// Kept for an operator to review; [`QueueHeldMail`] sends it.
    Held,
    /// The relay accepted it, or it was marked sent without delivery: it
    /// had no sender, or the transport was in mocked mode.
    Sent,
    /// The relay rejected it permanently.
    Failed,
    /// Not sent before `send_before_or_cancel`.
    Cancelled,
}

impl MailStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Held => "held",
            Self::Sent => "sent",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl Type<Postgres> for MailStatus {
    fn type_info() -> PgTypeInfo {
        <&str as Type<Postgres>>::type_info()
    }

    fn compatible(ty: &PgTypeInfo) -> bool {
        <&str as Type<Postgres>>::compatible(ty)
    }
}

impl Encode<'_, Postgres> for MailStatus {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<IsNull, BoxDynError> {
        <&str as Encode<Postgres>>::encode(self.as_str(), buf)
    }
}

impl<'r> Decode<'r, Postgres> for MailStatus {
    fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
        match <&str as Decode<Postgres>>::decode(value)? {
            "queued" => Ok(Self::Queued),
            "held" => Ok(Self::Held),
            "sent" => Ok(Self::Sent),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(format!("unknown mail status {other:?}").into()),
        }
    }
}

/// Store a message, unless the same request was stored already (see
/// [`InsertMail::idempotency_key`]). The stored message is returned as it is.
#[derive(Debug)]
pub struct InsertMail {
    /// When the caller decided to send this message (UTC), e.g. the time of
    /// the event it reacts to. Must be the same on every retry of the
    /// request: never the time `SendEmail` runs.
    pub requested_at: PrimitiveDateTime,
    pub send_to_address: EmailAddress,
    pub send_to_name: String,
    /// `None` marks the message sent at once, without delivering it.
    pub send_as: Option<SmtpSenderId>,
    pub title: String,
    pub message_content: MailContent,
    pub format: EmailFormat,
    /// `Queued` or `Held`. Ignored without a sender: the message is stored
    /// as `Sent`.
    pub status: MailStatus,
    pub send_before_or_cancel: Option<PrimitiveDateTime>,
}

/// Domain of the [`IdempotencyKey`] hash. Change the version whenever the
/// hashed fields change.
const IDEMPOTENCY_KEY_CONTEXT: &str = "Airi mailer.send_box idempotency key v1";

impl InsertMail {
    /// The hash of everything that makes this request distinct: when it was
    /// made, who receives the message, who sends it, and the message itself.
    /// The same message to several recipients, or to one recipient at two
    /// different times, gets different keys. `status` and
    /// `send_before_or_cancel` are delivery policy, not part of the request.
    pub fn idempotency_key(&self) -> IdempotencyKey {
        let mut hasher = blake3::Hasher::new_derive_key(IDEMPOTENCY_KEY_CONTEXT);
        hasher.update(
            &self
                .requested_at
                .assume_utc()
                .unix_timestamp_nanos()
                .to_le_bytes(),
        );
        // Tagged, so "no sender" can't collide with a sender's ID bytes.
        match self.send_as {
            Some(sender) => {
                hasher.update(&[1]);
                hasher.update(sender.0.as_bytes());
            }
            None => {
                hasher.update(&[0]);
            }
        }
        // Variable-length fields are length-prefixed, so bytes can't move
        // from one field to its neighbour without changing the key.
        for field in [
            self.send_to_address.as_ref(),
            self.send_to_name.as_str(),
            self.title.as_str(),
            self.message_content.0.as_str(),
            self.format.as_str(),
        ] {
            hasher.update(&(field.len() as u64).to_le_bytes());
            hasher.update(field.as_bytes());
        }
        IdempotencyKey(*hasher.finalize().as_bytes())
    }
}

#[derive(Debug)]
pub enum InsertMailOutcome {
    /// The message was stored now.
    Created(SendBoxEntity),
    /// A message with this idempotency key was stored before.
    Existing(SendBoxEntity),
}

impl Query for InsertMail {
    type Effect = Write<SendBoxEntity>;
}

impl Execute<PgSource> for InsertMail {
    type Output = InsertMailOutcome;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        let idempotency_key = self.idempotency_key();
        let status = if self.send_as.is_some() {
            self.status
        } else {
            MailStatus::Sent
        };
        let created = sqlx::query_as!(
            SendBoxEntity,
            r#"
            INSERT INTO "mailer"."send_box" (
                "idempotency_key", "send_to_address", "send_to_name", "send_as",
                "title", "message_content", "format", "status", "send_before_or_cancel",
                "queued_at", "sent_at"
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9,
                CASE WHEN $8 = 'queued' THEN now() AT TIME ZONE 'UTC' END,
                CASE WHEN $8 = 'sent' THEN now() AT TIME ZONE 'UTC' END
            )
            ON CONFLICT ("idempotency_key") DO NOTHING
            RETURNING
                "mail_id" AS "mail_id: MailId",
                "idempotency_key" AS "idempotency_key: IdempotencyKey",
                "send_to_address" AS "send_to_address: EmailAddress", "send_to_name",
                "send_as" AS "send_as: SmtpSenderId", "title",
                "message_content" AS "message_content: MailContent",
                "format" AS "format: EmailFormat", "status" AS "status: MailStatus",
                "failure_reply", "created_at", "queued_at", "attempted_at", "sent_at",
                "send_before_or_cancel"
            "#,
            idempotency_key as IdempotencyKey,
            self.send_to_address as EmailAddress,
            self.send_to_name,
            self.send_as as Option<SmtpSenderId>,
            self.title,
            self.message_content as MailContent,
            self.format as EmailFormat,
            status as MailStatus,
            self.send_before_or_cancel,
        )
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(mail) = created {
            return Ok(InsertMailOutcome::Created(mail));
        }

        // A separate statement: the insert's snapshot predates a conflicting
        // row committed by a concurrent request, this one sees it.
        let existing = sqlx::query_as!(
            SendBoxEntity,
            r#"
            SELECT
                "mail_id" AS "mail_id: MailId",
                "idempotency_key" AS "idempotency_key: IdempotencyKey",
                "send_to_address" AS "send_to_address: EmailAddress", "send_to_name",
                "send_as" AS "send_as: SmtpSenderId", "title",
                "message_content" AS "message_content: MailContent",
                "format" AS "format: EmailFormat", "status" AS "status: MailStatus",
                "failure_reply", "created_at", "queued_at", "attempted_at", "sent_at",
                "send_before_or_cancel"
            FROM "mailer"."send_box"
            WHERE "idempotency_key" = $1
            "#,
            idempotency_key as IdempotencyKey,
        )
        .fetch_one(conn)
        .await?;
        Ok(InsertMailOutcome::Existing(existing))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FindMailById {
    pub mail_id: MailId,
}

impl Query for FindMailById {
    type Effect = Read<SendBoxEntity>;
}

impl Execute<PgSource> for FindMailById {
    type Output = Option<SendBoxEntity>;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            SendBoxEntity,
            r#"
            SELECT
                "mail_id" AS "mail_id: MailId",
                "idempotency_key" AS "idempotency_key: IdempotencyKey",
                "send_to_address" AS "send_to_address: EmailAddress", "send_to_name",
                "send_as" AS "send_as: SmtpSenderId", "title",
                "message_content" AS "message_content: MailContent",
                "format" AS "format: EmailFormat", "status" AS "status: MailStatus",
                "failure_reply", "created_at", "queued_at", "attempted_at", "sent_at",
                "send_before_or_cancel"
            FROM "mailer"."send_box"
            WHERE "mail_id" = $1
            "#,
            self.mail_id as MailId,
        )
        .fetch_optional(conn)
        .await
    }
}

/// One page of messages, newest first.
#[derive(Debug, Clone, Copy)]
pub struct ListMail {
    /// Only messages in this status; `None` for every status.
    pub status: Option<MailStatus>,
    /// Only messages older than this one: the last ID of the previous page.
    pub before: Option<MailId>,
    pub limit: i64,
}

impl Query for ListMail {
    type Effect = Read<SendBoxEntity>;
}

impl Execute<PgSource> for ListMail {
    type Output = Vec<SendBoxEntity>;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            SendBoxEntity,
            r#"
            SELECT
                "mail_id" AS "mail_id: MailId",
                "idempotency_key" AS "idempotency_key: IdempotencyKey",
                "send_to_address" AS "send_to_address: EmailAddress", "send_to_name",
                "send_as" AS "send_as: SmtpSenderId", "title",
                "message_content" AS "message_content: MailContent",
                "format" AS "format: EmailFormat", "status" AS "status: MailStatus",
                "failure_reply", "created_at", "queued_at", "attempted_at", "sent_at",
                "send_before_or_cancel"
            FROM "mailer"."send_box"
            WHERE ($1::text IS NULL OR "status" = $1)
              AND ($2::bigint IS NULL OR "mail_id" < $2)
            ORDER BY "mail_id" DESC
            LIMIT $3
            "#,
            self.status as Option<MailStatus>,
            self.before as Option<MailId>,
            self.limit,
        )
        .fetch_all(conn)
        .await
    }
}

/// `queued` → `sent`. Outputs when it was marked sent (UTC), `None` if the
/// message is not queued.
#[derive(Debug, Clone, Copy)]
pub struct MarkMailSent {
    pub mail_id: MailId,
}

impl Query for MarkMailSent {
    type Effect = Write<SendBoxEntity>;
}

impl Execute<PgSource> for MarkMailSent {
    type Output = Option<PrimitiveDateTime>;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        let row = sqlx::query!(
            r#"
            UPDATE "mailer"."send_box"
            SET "status" = 'sent', "sent_at" = now() AT TIME ZONE 'UTC'
            WHERE "mail_id" = $1 AND "status" = 'queued'
            RETURNING "sent_at" AS "sent_at!"
            "#,
            self.mail_id as MailId,
        )
        .fetch_optional(conn)
        .await?;
        Ok(row.map(|row| row.sent_at))
    }
}

/// `queued` → `failed`, keeping the relay's reply. Outputs whether the
/// message was queued.
#[derive(Debug, Clone)]
pub struct MarkMailFailed {
    pub mail_id: MailId,
    pub failure_reply: String,
}

impl Query for MarkMailFailed {
    type Effect = Write<SendBoxEntity>;
}

impl Execute<PgSource> for MarkMailFailed {
    type Output = bool;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE "mailer"."send_box"
            SET "status" = 'failed', "failure_reply" = $2
            WHERE "mail_id" = $1 AND "status" = 'queued'
            "#,
            self.mail_id as MailId,
            self.failure_reply,
        )
        .execute(conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

/// `queued` or `held` → `cancelled`. Outputs whether the message was still
/// unsent.
#[derive(Debug, Clone, Copy)]
pub struct CancelMail {
    pub mail_id: MailId,
}

impl Query for CancelMail {
    type Effect = Write<SendBoxEntity>;
}

impl Execute<PgSource> for CancelMail {
    type Output = bool;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE "mailer"."send_box"
            SET "status" = 'cancelled'
            WHERE "mail_id" = $1 AND "status" IN ('queued', 'held')
            "#,
            self.mail_id as MailId,
        )
        .execute(conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

/// `held` → `queued`: an operator sends a held message. Outputs whether the
/// message was held.
#[derive(Debug, Clone, Copy)]
pub struct QueueHeldMail {
    pub mail_id: MailId,
}

impl Query for QueueHeldMail {
    type Effect = Write<SendBoxEntity>;
}

impl Execute<PgSource> for QueueHeldMail {
    type Output = bool;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE "mailer"."send_box"
            SET "status" = 'queued', "queued_at" = now() AT TIME ZONE 'UTC'
            WHERE "mail_id" = $1 AND "status" = 'held'
            "#,
            self.mail_id as MailId,
        )
        .execute(conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

/// Record that the transport is trying to deliver a queued message, so the
/// fallback job leaves it alone. Outputs whether the message was queued.
#[derive(Debug, Clone, Copy)]
pub struct MarkMailAttempted {
    pub mail_id: MailId,
}

impl Query for MarkMailAttempted {
    type Effect = Write<SendBoxEntity>;
}

impl Execute<PgSource> for MarkMailAttempted {
    type Output = bool;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE "mailer"."send_box"
            SET "attempted_at" = now() AT TIME ZONE 'UTC'
            WHERE "mail_id" = $1 AND "status" = 'queued'
            "#,
            self.mail_id as MailId,
        )
        .execute(conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

/// `queued` → `failed` for every message queued before `queued_before` (UTC)
/// that the transport never attempted: its `EmailQueued` was lost. Outputs
/// how many messages failed.
#[derive(Debug, Clone)]
pub struct FailUnattemptedMail {
    pub queued_before: PrimitiveDateTime,
    pub failure_reply: String,
}

impl Query for FailUnattemptedMail {
    type Effect = Write<SendBoxEntity>;
}

impl Execute<PgSource> for FailUnattemptedMail {
    type Output = u64;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE "mailer"."send_box"
            SET "status" = 'failed', "failure_reply" = $2
            WHERE "status" = 'queued' AND "attempted_at" IS NULL AND "queued_at" < $1
            "#,
            self.queued_before,
            self.failure_reply,
        )
        .execute(conn)
        .await?;
        Ok(result.rows_affected())
    }
}

/// Delete the bodies of messages created before `created_before` (UTC).
/// Queued and held messages keep theirs: they are still to be sent or read.
/// Outputs how many bodies were deleted.
#[derive(Debug, Clone, Copy)]
pub struct DeleteMailContentBefore {
    pub created_before: PrimitiveDateTime,
}

impl Query for DeleteMailContentBefore {
    type Effect = Write<SendBoxEntity>;
}

impl Execute<PgSource> for DeleteMailContentBefore {
    type Output = u64;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        let result = sqlx::query!(
            r#"
            UPDATE "mailer"."send_box"
            SET "message_content" = NULL
            WHERE "created_at" < $1
              AND "message_content" IS NOT NULL
              AND "status" NOT IN ('queued', 'held')
            "#,
            self.created_before,
        )
        .execute(conn)
        .await?;
        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Date, Month, Time};
    use uuid::Uuid;

    fn request(title: &str, content: &str) -> InsertMail {
        InsertMail {
            requested_at: PrimitiveDateTime::new(
                Date::from_calendar_date(2026, Month::November, 25).unwrap(),
                Time::MIDNIGHT,
            ),
            send_to_address: "member@example.com".parse().unwrap(),
            send_to_name: String::new(),
            send_as: Some(SmtpSenderId(Uuid::nil())),
            title: title.to_owned(),
            message_content: MailContent(content.to_owned()),
            format: EmailFormat::Plain,
            status: MailStatus::Queued,
            send_before_or_cancel: None,
        }
    }

    #[test]
    fn bytes_moved_between_fields_change_the_key() {
        assert_ne!(
            request("ab", "c").idempotency_key(),
            request("a", "bc").idempotency_key(),
        );
    }
}
