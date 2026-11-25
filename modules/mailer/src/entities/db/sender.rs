//! The `mailer.smtp_sender` table: one SMTP account per From address.
//!
//! Credentials are stored encrypted and bound to the sender's ID (see
//! [`SmtpSenderId::username_associated_data`]), so a ciphertext copied into
//! another row, or from one column into the other, fails to decrypt.

use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::postgres::{PgArgumentBuffer, PgTypeInfo, PgValueRef};
use sqlx::{Decode, Encode, PgConnection, Postgres, Type};
use time::PrimitiveDateTime;
use type_safe_data::encryption::{DefaultSymmetricEncryption, EncryptedString};
use type_safe_data::mail::EmailAddress;
use uuid::Uuid;
use wakuwaku::entity::sqlx::backend::PgSource;
use wakuwaku::entity::{Entity, Execute, Query, Read, Write};

/// An encrypted SMTP username or password.
pub type SmtpSecret = EncryptedString<DefaultSymmetricEncryption>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, sqlx::Type)]
#[sqlx(transparent)]
pub struct SmtpSenderId(pub Uuid);

impl SmtpSenderId {
    /// A fresh, time-ordered ID. Generate it before encrypting the new
    /// sender's credentials.
    pub fn generate() -> Self {
        Self(Uuid::now_v7())
    }

    /// Associated data of this sender's encrypted username.
    pub fn username_associated_data(self) -> Vec<u8> {
        self.associated_data(b"mailer.smtp_sender.username:")
    }

    /// Associated data of this sender's encrypted password.
    pub fn password_associated_data(self) -> Vec<u8> {
        self.associated_data(b"mailer.smtp_sender.password:")
    }

    fn associated_data(self, column: &[u8]) -> Vec<u8> {
        [column, self.0.as_bytes()].concat()
    }
}

/// How the connection to the relay is secured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmtpTlsMode {
    /// Upgrade a plain connection with `STARTTLS`, usually on port 587.
    StartTls,
    /// TLS from the first byte, usually on port 465.
    ImplicitTls,
    /// No encryption. Only for a local mail catcher: credentials and
    /// messages cross the network in the clear.
    Plaintext,
}

impl SmtpTlsMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StartTls => "starttls",
            Self::ImplicitTls => "implicit_tls",
            Self::Plaintext => "plaintext",
        }
    }
}

impl Type<Postgres> for SmtpTlsMode {
    fn type_info() -> PgTypeInfo {
        <&str as Type<Postgres>>::type_info()
    }

    fn compatible(ty: &PgTypeInfo) -> bool {
        <&str as Type<Postgres>>::compatible(ty)
    }
}

impl Encode<'_, Postgres> for SmtpTlsMode {
    fn encode_by_ref(&self, buf: &mut PgArgumentBuffer) -> Result<IsNull, BoxDynError> {
        <&str as Encode<Postgres>>::encode(self.as_str(), buf)
    }
}

impl<'r> Decode<'r, Postgres> for SmtpTlsMode {
    fn decode(value: PgValueRef<'r>) -> Result<Self, BoxDynError> {
        match <&str as Decode<Postgres>>::decode(value)? {
            "starttls" => Ok(Self::StartTls),
            "implicit_tls" => Ok(Self::ImplicitTls),
            "plaintext" => Ok(Self::Plaintext),
            other => Err(format!("unknown SMTP TLS mode {other:?}").into()),
        }
    }
}

/// A row of `mailer.smtp_sender`.
///
/// `username_encrypted` and `password_encrypted` are both set or both `None`
/// (a relay without authentication).
#[derive(Debug, Clone)]
pub struct SmtpSenderEntity {
    pub id: SmtpSenderId,
    /// Unique, stable name callers pick the sender by, e.g. `noreply`:
    /// 1–63 of `a-z`, `0-9`, `_`, `-`, starting with a letter or digit.
    pub name: String,
    /// The From address.
    pub address: EmailAddress,
    pub host: String,
    /// 1–65535.
    pub port: i32,
    pub tls_mode: SmtpTlsMode,
    pub username_encrypted: Option<SmtpSecret>,
    pub password_encrypted: Option<SmtpSecret>,
    /// A disabled sender rejects new messages.
    pub enabled: bool,
    pub remark: String,
    /// UTC.
    pub created_at: PrimitiveDateTime,
    /// UTC.
    pub updated_at: PrimitiveDateTime,
}

impl Entity for SmtpSenderEntity {}

/// Encrypted SMTP credentials, written as a pair.
#[derive(Debug, Clone)]
pub struct SmtpCredentials {
    pub username_encrypted: SmtpSecret,
    pub password_encrypted: SmtpSecret,
}

impl SmtpCredentials {
    fn into_columns(credentials: Option<Self>) -> (Option<SmtpSecret>, Option<SmtpSecret>) {
        credentials.map_or((None, None), |c| {
            (Some(c.username_encrypted), Some(c.password_encrypted))
        })
    }
}

/// Insert a sender. Fails with a unique violation if `name` is taken.
#[derive(Debug)]
pub struct InsertSmtpSender {
    /// From [`SmtpSenderId::generate`], the ID `credentials` are bound to.
    pub id: SmtpSenderId,
    pub name: String,
    pub address: EmailAddress,
    pub host: String,
    pub port: i32,
    pub tls_mode: SmtpTlsMode,
    pub credentials: Option<SmtpCredentials>,
    pub enabled: bool,
    pub remark: String,
}

impl Query for InsertSmtpSender {
    type Effect = Write<SmtpSenderEntity>;
}

impl Execute<PgSource> for InsertSmtpSender {
    type Output = SmtpSenderEntity;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        let (username, password) = SmtpCredentials::into_columns(self.credentials);
        sqlx::query_as!(
            SmtpSenderEntity,
            r#"
            INSERT INTO "mailer"."smtp_sender" (
                "id", "name", "address", "host", "port", "tls_mode",
                "username_encrypted", "password_encrypted", "enabled", "remark"
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING
                "id" AS "id: SmtpSenderId", "name", "address" AS "address: EmailAddress",
                "host", "port", "tls_mode" AS "tls_mode: SmtpTlsMode",
                "username_encrypted" AS "username_encrypted: SmtpSecret",
                "password_encrypted" AS "password_encrypted: SmtpSecret",
                "enabled", "remark", "created_at", "updated_at"
            "#,
            self.id as SmtpSenderId,
            self.name,
            self.address as EmailAddress,
            self.host,
            self.port,
            self.tls_mode as SmtpTlsMode,
            username as Option<SmtpSecret>,
            password as Option<SmtpSecret>,
            self.enabled,
            self.remark,
        )
        .fetch_one(conn)
        .await
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FindSmtpSenderById {
    pub id: SmtpSenderId,
}

impl Query for FindSmtpSenderById {
    type Effect = Read<SmtpSenderEntity>;
}

impl Execute<PgSource> for FindSmtpSenderById {
    type Output = Option<SmtpSenderEntity>;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            SmtpSenderEntity,
            r#"
            SELECT
                "id" AS "id: SmtpSenderId", "name", "address" AS "address: EmailAddress",
                "host", "port", "tls_mode" AS "tls_mode: SmtpTlsMode",
                "username_encrypted" AS "username_encrypted: SmtpSecret",
                "password_encrypted" AS "password_encrypted: SmtpSecret",
                "enabled", "remark", "created_at", "updated_at"
            FROM "mailer"."smtp_sender"
            WHERE "id" = $1
            "#,
            self.id as SmtpSenderId,
        )
        .fetch_optional(conn)
        .await
    }
}

/// Find the sender a calling module names.
#[derive(Debug, Clone)]
pub struct FindSmtpSenderByName {
    pub name: String,
}

impl Query for FindSmtpSenderByName {
    type Effect = Read<SmtpSenderEntity>;
}

impl Execute<PgSource> for FindSmtpSenderByName {
    type Output = Option<SmtpSenderEntity>;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            SmtpSenderEntity,
            r#"
            SELECT
                "id" AS "id: SmtpSenderId", "name", "address" AS "address: EmailAddress",
                "host", "port", "tls_mode" AS "tls_mode: SmtpTlsMode",
                "username_encrypted" AS "username_encrypted: SmtpSecret",
                "password_encrypted" AS "password_encrypted: SmtpSecret",
                "enabled", "remark", "created_at", "updated_at"
            FROM "mailer"."smtp_sender"
            WHERE "name" = $1
            "#,
            self.name,
        )
        .fetch_optional(conn)
        .await
    }
}

/// Every sender, by name.
#[derive(Debug, Clone, Copy)]
pub struct ListSmtpSenders;

impl Query for ListSmtpSenders {
    type Effect = Read<SmtpSenderEntity>;
}

impl Execute<PgSource> for ListSmtpSenders {
    type Output = Vec<SmtpSenderEntity>;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            SmtpSenderEntity,
            r#"
            SELECT
                "id" AS "id: SmtpSenderId", "name", "address" AS "address: EmailAddress",
                "host", "port", "tls_mode" AS "tls_mode: SmtpTlsMode",
                "username_encrypted" AS "username_encrypted: SmtpSecret",
                "password_encrypted" AS "password_encrypted: SmtpSecret",
                "enabled", "remark", "created_at", "updated_at"
            FROM "mailer"."smtp_sender"
            ORDER BY "name"
            "#,
        )
        .fetch_all(conn)
        .await
    }
}

/// Replace every setting of a sender except its credentials. Outputs the
/// updated row, `None` if the sender does not exist.
#[derive(Debug, Clone)]
pub struct UpdateSmtpSender {
    pub id: SmtpSenderId,
    pub name: String,
    pub address: EmailAddress,
    pub host: String,
    pub port: i32,
    pub tls_mode: SmtpTlsMode,
    pub enabled: bool,
    pub remark: String,
}

impl Query for UpdateSmtpSender {
    type Effect = Write<SmtpSenderEntity>;
}

impl Execute<PgSource> for UpdateSmtpSender {
    type Output = Option<SmtpSenderEntity>;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            SmtpSenderEntity,
            r#"
            UPDATE "mailer"."smtp_sender"
            SET "name" = $2, "address" = $3, "host" = $4, "port" = $5, "tls_mode" = $6,
                "enabled" = $7, "remark" = $8,
                "updated_at" = now() AT TIME ZONE 'UTC'
            WHERE "id" = $1
            RETURNING
                "id" AS "id: SmtpSenderId", "name", "address" AS "address: EmailAddress",
                "host", "port", "tls_mode" AS "tls_mode: SmtpTlsMode",
                "username_encrypted" AS "username_encrypted: SmtpSecret",
                "password_encrypted" AS "password_encrypted: SmtpSecret",
                "enabled", "remark", "created_at", "updated_at"
            "#,
            self.id as SmtpSenderId,
            self.name,
            self.address as EmailAddress,
            self.host,
            self.port,
            self.tls_mode as SmtpTlsMode,
            self.enabled,
            self.remark,
        )
        .fetch_optional(conn)
        .await
    }
}

/// Replace a sender's credentials (`None` removes them). Outputs whether the
/// sender exists.
#[derive(Debug, Clone)]
pub struct SetSmtpSenderCredentials {
    /// The ID the new credentials are bound to.
    pub id: SmtpSenderId,
    pub credentials: Option<SmtpCredentials>,
}

impl Query for SetSmtpSenderCredentials {
    type Effect = Write<SmtpSenderEntity>;
}

impl Execute<PgSource> for SetSmtpSenderCredentials {
    type Output = bool;
    type Error = sqlx::Error;

    #[tracing::instrument(skip_all, err)]
    async fn execute(self, conn: &mut PgConnection) -> Result<Self::Output, Self::Error> {
        let (username, password) = SmtpCredentials::into_columns(self.credentials);
        let result = sqlx::query!(
            r#"
            UPDATE "mailer"."smtp_sender"
            SET "username_encrypted" = $2, "password_encrypted" = $3,
                "updated_at" = now() AT TIME ZONE 'UTC'
            WHERE "id" = $1
            "#,
            self.id as SmtpSenderId,
            username as Option<SmtpSecret>,
            password as Option<SmtpSecret>,
        )
        .execute(conn)
        .await?;
        Ok(result.rows_affected() > 0)
    }
}
