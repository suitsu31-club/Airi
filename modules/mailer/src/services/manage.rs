//! `MailerManageService`: how operators manage senders.
//!
//! Every input that carries operator-typed values carries them proven:
//! sender settings by [`VALID_SENDER_SETTINGS`], credentials by
//! [`VALID_SMTP_LOGIN`]. The caller (an RPC handler, a `manage-tool`
//! command) runs the check and maps a denial to its own error:
//!
//! ```ignore
//! use wakuwaku::proof::ValidateError;
//!
//! let settings = match VALID_SENDER_SETTINGS.prove(settings) {
//!     Ok(settings) => settings,
//!     Err(ValidateError::Deny(reason)) => return Err(/* bad input: `reason` */),
//!     Err(ValidateError::IoError(never)) => match never {},
//! };
//! manage.process(UpdateSender { id, settings }).await?;
//! ```
//!
//! Credentials are write-only: they are encrypted here with
//! `MAILER_SECRET_KEY` and no output carries them back, not even encrypted.
//! [`RotateSecretKey`] is the only operation that decrypts them.

use crate::entities::db::sender::{
    FindSmtpSenderById, InsertSmtpSender, ListSmtpSenders, ReencryptedCredentials,
    ReplaceSmtpSenderCredentials, SetSmtpSenderCredentials, SetSmtpSenderEnabled, SmtpCredentials,
    SmtpSecret, SmtpSenderEntity, UpdateSmtpSender,
};
use anyhow::anyhow;
use base::Error;
use kanau::processor::Processor;
use std::convert::Infallible;
use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;
use time::PrimitiveDateTime;
use type_safe_data::encryption::{EncryptionError, Key256};
use type_safe_data::mail::EmailAddress;
use wakuwaku::entity::sqlx::backend::PgSource;
use wakuwaku::entity::{Db, ReadWrite};
use wakuwaku::proof::{DeniedReason, Proven, PureVerifier, ValidateError};
use zeroize::Zeroizing;

pub use crate::entities::db::sender::{SmtpSenderId, SmtpTlsMode};

/// Creates, changes and lists senders, and owns the key their credentials
/// are encrypted with.
#[derive(Clone)]
pub struct MailerManageService {
    db: Db<PgSource, ReadWrite>,
    /// `None` without `MAILER_SECRET_KEY`: senders without credentials can
    /// still be managed.
    secret_key: Option<Arc<Key256>>,
}

impl MailerManageService {
    /// `secret_key` is the key from
    /// [`secret_key_from_env`](crate::utils::secret_key::secret_key_from_env).
    pub const fn new(db: Db<PgSource, ReadWrite>, secret_key: Option<Arc<Key256>>) -> Self {
        Self { db, secret_key }
    }
}

/// The name is not 1–63 of `a-z`, `0-9`, `_`, `-` starting with a letter or
/// digit.
pub static INVALID_SENDER_NAME: DeniedReason =
    DeniedReason("sender name must be 1-63 of a-z, 0-9, '_' and '-', starting with a-z or 0-9");
/// The host is empty, longer than 253 bytes, or not printable ASCII.
pub static INVALID_SMTP_HOST: DeniedReason =
    DeniedReason("SMTP host must be 1-253 printable ASCII characters without spaces");
/// The port is 0.
pub static INVALID_SMTP_PORT: DeniedReason = DeniedReason("SMTP port must be 1-65535");
/// The username or the password is empty or contains a NUL character.
pub static INVALID_SMTP_LOGIN: DeniedReason =
    DeniedReason("SMTP username and password must be non-empty and contain no NUL character");

/// Everything about a sender an operator sets, except its credentials and
/// whether it is enabled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderSettings {
    /// Unique, stable name callers pick the sender by, e.g. `noreply`.
    pub name: String,
    /// The From address.
    pub address: EmailAddress,
    pub host: String,
    pub port: u16,
    pub tls_mode: SmtpTlsMode,
    pub remark: String,
}

/// Evidence that [`SenderSettings`] fit the `mailer.smtp_sender` table.
#[derive(Debug)]
pub struct ValidSenderSettings(());

fn check_sender_settings(
    settings: &SenderSettings,
) -> Result<ValidSenderSettings, ValidateError<Infallible>> {
    if !is_sender_name(&settings.name) {
        return Err(ValidateError::Deny(INVALID_SENDER_NAME));
    }
    if !is_host(&settings.host) {
        return Err(ValidateError::Deny(INVALID_SMTP_HOST));
    }
    if settings.port == 0 {
        return Err(ValidateError::Deny(INVALID_SMTP_PORT));
    }
    Ok(ValidSenderSettings(()))
}

fn is_sender_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    let allowed = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-');
    bytes.len() <= 63
        && bytes
            .first()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && bytes.iter().all(allowed)
}

fn is_host(host: &str) -> bool {
    (1..=253).contains(&host.len()) && host.bytes().all(|b| b.is_ascii_graphic())
}

/// The check for [`SenderSettings`].
pub static VALID_SENDER_SETTINGS: PureVerifier<SenderSettings, ValidSenderSettings> =
    PureVerifier::new(check_sender_settings);

/// SMTP credentials in plaintext, as an operator types them. Wiped from
/// memory on drop; `Debug` shows neither value.
#[derive(Clone)]
pub struct SmtpLogin {
    pub username: Zeroizing<String>,
    pub password: Zeroizing<String>,
}

impl Debug for SmtpLogin {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("SmtpLogin(..)")
    }
}

/// Evidence that an [`SmtpLogin`] can be sent to a relay.
#[derive(Debug)]
pub struct ValidSmtpLogin(());

fn check_smtp_login(login: &SmtpLogin) -> Result<ValidSmtpLogin, ValidateError<Infallible>> {
    let usable = |value: &str| !value.is_empty() && !value.contains('\0');
    if usable(&login.username) && usable(&login.password) {
        Ok(ValidSmtpLogin(()))
    } else {
        Err(ValidateError::Deny(INVALID_SMTP_LOGIN))
    }
}

/// The check for [`SmtpLogin`].
pub static VALID_SMTP_LOGIN: PureVerifier<SmtpLogin, ValidSmtpLogin> =
    PureVerifier::new(check_smtp_login);

/// A sender as operators see it: its credentials are reduced to whether it
/// has any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SenderView {
    pub id: SmtpSenderId,
    pub name: String,
    pub address: EmailAddress,
    pub host: String,
    pub port: u16,
    pub tls_mode: SmtpTlsMode,
    /// `false`: the relay is used without authentication.
    pub has_credentials: bool,
    /// A disabled sender rejects new messages.
    pub enabled: bool,
    pub remark: String,
    /// UTC.
    pub created_at: PrimitiveDateTime,
    /// UTC.
    pub updated_at: PrimitiveDateTime,
}

impl TryFrom<SmtpSenderEntity> for SenderView {
    type Error = Error;

    fn try_from(sender: SmtpSenderEntity) -> Result<Self, Error> {
        let port = u16::try_from(sender.port).map_err(|_| {
            Error::BusinessPanic(anyhow!("SMTP port {} is out of range", sender.port))
        })?;
        Ok(Self {
            id: sender.id,
            name: sender.name,
            address: sender.address,
            host: sender.host,
            port,
            tls_mode: sender.tls_mode,
            has_credentials: sender.username_encrypted.is_some(),
            enabled: sender.enabled,
            remark: sender.remark,
            created_at: sender.created_at,
            updated_at: sender.updated_at,
        })
    }
}

/// Add a sender. Fails with `InvalidInput` if its name is taken.
#[derive(Debug)]
pub struct CreateSender {
    pub settings: Proven<SenderSettings, ValidSenderSettings>,
    /// `None`: the relay is used without authentication.
    pub login: Option<Proven<SmtpLogin, ValidSmtpLogin>>,
    pub enabled: bool,
}

impl Processor<CreateSender> for MailerManageService {
    type Output = SenderView;
    type Error = Error;

    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: CreateSender) -> Result<SenderView, Error> {
        // The credentials are bound to the ID, so it exists before the row.
        let id = SmtpSenderId::generate();
        let credentials = match &input.login {
            Some(login) => Some(self.encrypt(id, login.subject())?),
            None => None,
        };
        let settings = input.settings.subject();
        self.db
            .process(InsertSmtpSender {
                id,
                name: settings.name.clone(),
                address: settings.address.clone(),
                host: settings.host.clone(),
                port: i32::from(settings.port),
                tls_mode: settings.tls_mode,
                credentials,
                enabled: input.enabled,
                remark: settings.remark.clone(),
            })
            .await
            .map_err(name_taken_is_invalid)?
            .try_into()
    }
}

/// Replace a sender's settings; its credentials and enabled flag stay.
/// Fails with `NotFound` for an unknown sender, and with `InvalidInput` if
/// the new name is taken.
#[derive(Debug)]
pub struct UpdateSender {
    pub id: SmtpSenderId,
    pub settings: Proven<SenderSettings, ValidSenderSettings>,
}

impl Processor<UpdateSender> for MailerManageService {
    type Output = SenderView;
    type Error = Error;

    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: UpdateSender) -> Result<SenderView, Error> {
        let settings = input.settings.subject();
        self.db
            .process(UpdateSmtpSender {
                id: input.id,
                name: settings.name.clone(),
                address: settings.address.clone(),
                host: settings.host.clone(),
                port: i32::from(settings.port),
                tls_mode: settings.tls_mode,
                remark: settings.remark.clone(),
            })
            .await
            .map_err(name_taken_is_invalid)?
            .ok_or(Error::NotFound)?
            .try_into()
    }
}

/// Enable or disable a sender. A disabled sender rejects new messages, and
/// the transport fails the messages it still has queued. Fails with
/// `NotFound` for an unknown sender.
#[derive(Debug, Clone, Copy)]
pub struct SetSenderEnabled {
    pub id: SmtpSenderId,
    pub enabled: bool,
}

impl Processor<SetSenderEnabled> for MailerManageService {
    type Output = SenderView;
    type Error = Error;

    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: SetSenderEnabled) -> Result<SenderView, Error> {
        self.db
            .process(SetSmtpSenderEnabled {
                id: input.id,
                enabled: input.enabled,
            })
            .await?
            .ok_or(Error::NotFound)?
            .try_into()
    }
}

/// Replace a sender's credentials, or remove them with `None`. Fails with
/// `NotFound` for an unknown sender.
#[derive(Debug)]
pub struct SetSenderLogin {
    pub id: SmtpSenderId,
    pub login: Option<Proven<SmtpLogin, ValidSmtpLogin>>,
}

impl Processor<SetSenderLogin> for MailerManageService {
    type Output = ();
    type Error = Error;

    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: SetSenderLogin) -> Result<(), Error> {
        let credentials = match &input.login {
            Some(login) => Some(self.encrypt(input.id, login.subject())?),
            None => None,
        };
        let exists = self
            .db
            .process(SetSmtpSenderCredentials {
                id: input.id,
                credentials,
            })
            .await?;
        if exists { Ok(()) } else { Err(Error::NotFound) }
    }
}

/// One sender. Fails with `NotFound` for an unknown sender.
#[derive(Debug, Clone, Copy)]
pub struct FindSender {
    pub id: SmtpSenderId,
}

impl Processor<FindSender> for MailerManageService {
    type Output = SenderView;
    type Error = Error;

    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: FindSender) -> Result<SenderView, Error> {
        self.db
            .process(FindSmtpSenderById { id: input.id })
            .await?
            .ok_or(Error::NotFound)?
            .try_into()
    }
}

/// Every sender, by name.
#[derive(Debug, Clone, Copy)]
pub struct ListSenders;

impl Processor<ListSenders> for MailerManageService {
    type Output = Vec<SenderView>;
    type Error = Error;

    #[tracing::instrument(skip_all, err)]
    async fn process(&self, _: ListSenders) -> Result<Vec<SenderView>, Error> {
        self.db
            .process(ListSmtpSenders)
            .await?
            .into_iter()
            .map(SenderView::try_from)
            .collect()
    }
}

/// Re-encrypt every sender's credentials from `old` to `new`.
///
/// Run it while the application still uses `old`, then switch
/// `MAILER_SECRET_KEY` to `new`. It can run again at any time: credentials
/// already under `new` are left alone, so a run that reported conflicts is
/// finished by running it once more. Credentials under neither key abort the
/// run before anything changes.
#[derive(Debug)]
pub struct RotateSecretKey {
    pub old: Key256,
    pub new: Key256,
}

/// What [`RotateSecretKey`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyRotation {
    /// Senders whose credentials are now under the new key.
    pub rotated: u64,
    /// Senders whose credentials were under the new key already.
    pub already_rotated: u64,
    /// Senders whose credentials changed while the rotation ran, and were
    /// left as the change wrote them. Run the rotation again.
    pub conflicted: u64,
}

impl Processor<RotateSecretKey> for MailerManageService {
    type Output = KeyRotation;
    type Error = Error;

    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: RotateSecretKey) -> Result<KeyRotation, Error> {
        let mut rows = Vec::new();
        let mut already_rotated: u64 = 0;
        for sender in self.db.process(ListSmtpSenders).await? {
            let (Some(username), Some(password)) =
                (sender.username_encrypted, sender.password_encrypted)
            else {
                continue;
            };
            let old = SmtpCredentials {
                username_encrypted: username,
                password_encrypted: password,
            };
            let login = match decrypt(&input.old, sender.id, &old) {
                Ok(login) => login,
                Err(EncryptionError::Decrypt) if decrypt(&input.new, sender.id, &old).is_ok() => {
                    already_rotated = already_rotated.saturating_add(1);
                    continue;
                }
                Err(error) => {
                    return Err(Error::BusinessPanic(anyhow!(
                        "the credentials of sender {} are under neither key: {error}",
                        sender.id.0
                    )));
                }
            };
            let new = encrypt(&input.new, sender.id, &login)?;
            rows.push(ReencryptedCredentials {
                id: sender.id,
                old,
                new,
            });
        }

        let attempted = rows.len() as u64;
        let rotated = if rows.is_empty() {
            0
        } else {
            self.db
                .process(ReplaceSmtpSenderCredentials { rows })
                .await?
        };
        Ok(KeyRotation {
            rotated,
            already_rotated,
            conflicted: attempted.saturating_sub(rotated),
        })
    }
}

impl MailerManageService {
    /// Encrypt `login` for the sender `id` with `MAILER_SECRET_KEY`.
    fn encrypt(&self, id: SmtpSenderId, login: &SmtpLogin) -> Result<SmtpCredentials, Error> {
        let key = self.secret_key.as_deref().ok_or_else(|| {
            Error::BusinessPanic(anyhow!(
                "MAILER_SECRET_KEY is unset; it is needed to store SMTP credentials"
            ))
        })?;
        encrypt(key, id, login)
    }
}

fn encrypt(key: &Key256, id: SmtpSenderId, login: &SmtpLogin) -> Result<SmtpCredentials, Error> {
    let seal = |plaintext: &str, associated_data: Vec<u8>| {
        SmtpSecret::encrypt(key, plaintext, &associated_data)
            .map_err(|error| Error::Io(error.into()))
    };
    Ok(SmtpCredentials {
        username_encrypted: seal(&login.username, id.username_associated_data())?,
        password_encrypted: seal(&login.password, id.password_associated_data())?,
    })
}

fn decrypt(
    key: &Key256,
    id: SmtpSenderId,
    credentials: &SmtpCredentials,
) -> Result<SmtpLogin, EncryptionError> {
    Ok(SmtpLogin {
        username: credentials
            .username_encrypted
            .decrypt(key, &id.username_associated_data())?,
        password: credentials
            .password_encrypted
            .decrypt(key, &id.password_associated_data())?,
    })
}

/// A taken sender name is the operator's mistake, not a database failure.
fn name_taken_is_invalid(error: sqlx::Error) -> Error {
    match &error {
        sqlx::Error::Database(db) if db.is_unique_violation() => Error::InvalidInput,
        _ => Error::Database(error),
    }
}
