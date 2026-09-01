//! Multi-factor authentication (TOTP) service.
//!
//! Owns enrollment (secret generation, QR/URI provisioning, recovery-code
//! minting), teardown, status, and the second-factor exchange that turns an
//! MFA-login token into a session. Returns domain types; the [`crate::rpc`]
//! edge adapts them to the wire.

use crate::entities::db::account::{AccountId, FindAccountById};
use crate::entities::db::sessions::SessionId;
use crate::entities::db::totp::{
    ConsumeRecoveryCode, CountRecoveryCodesByUserId, CreateUserTotp, DeleteRecoveryCodesByUserId,
    DeleteUserTotpByUserId, FindUserTotpByUserId, StoreRecoveryCodes,
};
use crate::entities::redis::mfa_login_token::{MfaLoginToken, mfa_login_token_key};
use crate::entities::redis::pending_totp::{PendingTotpSetup, pending_totp_key};
use crate::entities::redis::session_cache::security_from_u8;
use crate::services::session::{CreateSession, SessionService};
use crate::utils::datetime::{now_primitive, to_unix};
use crate::utils::totp::{
    build_totp, generate_recovery_codes, generate_secret_bytes, hash_recovery_code, otpauth_uri,
    qr_png_base64, secret_base32, verify_totp,
};
use base::events::UserLoginEvent;
use kanau::processor::Processor;
use std::time::Duration;
use wakuwaku::amqp::{AmqpMessageSend, AmqpPool};
use wakuwaku::redis::{KeyValue, KeyValueRead, KeyValueWrite, RedisConnection};
use wakuwaku::sqlx::DatabaseProcessor;

/// Enrollment/verification service for TOTP-based MFA.
#[derive(Clone)]
pub struct MfaService {
    pub db: DatabaseProcessor,
    pub redis: RedisConnection,
    pub session: SessionService,
    pub mq: AmqpPool,
}

/// The material a client needs to finish TOTP enrollment.
#[derive(Debug, Clone)]
pub struct TotpEnrollmentStart {
    pub secret_base32: String,
    pub otpauth_uri: String,
    pub qr_png_base64: String,
}

/// Begin TOTP enrollment for a user.
pub struct StartTotpEnrollment {
    pub user_id: AccountId,
}

impl Processor<StartTotpEnrollment> for MfaService {
    type Output = TotpEnrollmentStart;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:StartTotpEnrollment")]
    async fn process(&self, input: StartTotpEnrollment) -> Result<Self::Output, Self::Error> {
        let account = self
            .db
            .process(FindAccountById { id: input.user_id })
            .await?
            .ok_or(wakuwaku::Error::NotFound)?;

        let secret = generate_secret_bytes();
        let totp = build_totp(&secret, &account.email)?;
        let start = TotpEnrollmentStart {
            secret_base32: secret_base32(&secret),
            otpauth_uri: otpauth_uri(&totp)?,
            qr_png_base64: qr_png_base64(&totp)?,
        };

        PendingTotpSetup {
            user_id: input.user_id.0,
            secret: secret.into_boxed_slice(),
        }
        .write_with_ttl(&mut self.redis.clone(), Duration::from_secs(300))
        .await?;

        Ok(start)
    }
}

/// Outcome of [`FinishTotpEnrollment`].
#[derive(Debug, Clone)]
pub enum FinishTotpEnrollmentResult {
    /// Enrollment succeeded; carries the one-time recovery codes (shown once).
    Success(Vec<String>),
    /// The submitted code did not match the pending secret.
    InvalidCode,
    /// No pending enrollment exists (expired or never started).
    NoPending,
    /// TOTP is already enabled for this user.
    AlreadyEnabled,
}

/// Confirm the first TOTP code and enable MFA.
pub struct FinishTotpEnrollment {
    pub user_id: AccountId,
    pub code: String,
}

impl Processor<FinishTotpEnrollment> for MfaService {
    type Output = FinishTotpEnrollmentResult;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:FinishTotpEnrollment")]
    async fn process(&self, input: FinishTotpEnrollment) -> Result<Self::Output, Self::Error> {
        if self
            .db
            .process(FindUserTotpByUserId {
                user_id: input.user_id.0,
            })
            .await?
            .is_some()
        {
            return Ok(FinishTotpEnrollmentResult::AlreadyEnabled);
        }

        let Some(pending) =
            PendingTotpSetup::read(&mut self.redis.clone(), pending_totp_key(input.user_id.0))
                .await?
        else {
            return Ok(FinishTotpEnrollmentResult::NoPending);
        };

        if !verify_totp(&pending.secret, &input.code) {
            return Ok(FinishTotpEnrollmentResult::InvalidCode);
        }

        PendingTotpSetup::delete(&mut self.redis.clone(), pending_totp_key(input.user_id.0))
            .await?;

        self.db
            .process(CreateUserTotp {
                user_id: input.user_id.0,
                secret: pending.secret.into_vec(),
            })
            .await?;

        let codes = generate_recovery_codes();
        let code_hashes = codes.iter().map(|c| hash_recovery_code(c)).collect();
        self.db
            .process(StoreRecoveryCodes {
                user_id: input.user_id.0,
                code_hashes,
            })
            .await?;

        Ok(FinishTotpEnrollmentResult::Success(codes))
    }
}

/// Outcome of [`DisableMfa`].
#[derive(Debug, Clone)]
pub enum DisableMfaResult {
    Success,
    InvalidCode,
    NotEnabled,
}

/// Disable MFA for a user after verifying a current code.
pub struct DisableMfa {
    pub user_id: AccountId,
    pub code: String,
}

impl Processor<DisableMfa> for MfaService {
    type Output = DisableMfaResult;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:DisableMfa")]
    async fn process(&self, input: DisableMfa) -> Result<Self::Output, Self::Error> {
        let Some(totp) = self
            .db
            .process(FindUserTotpByUserId {
                user_id: input.user_id.0,
            })
            .await?
        else {
            return Ok(DisableMfaResult::NotEnabled);
        };

        let ok = verify_totp(&totp.secret, &input.code)
            || self
                .db
                .process(ConsumeRecoveryCode {
                    user_id: input.user_id.0,
                    code_hash: hash_recovery_code(&input.code),
                })
                .await?;
        if !ok {
            return Ok(DisableMfaResult::InvalidCode);
        }

        self.db
            .process(DeleteUserTotpByUserId {
                user_id: input.user_id.0,
            })
            .await?;
        self.db
            .process(DeleteRecoveryCodesByUserId {
                user_id: input.user_id.0,
            })
            .await?;

        Ok(DisableMfaResult::Success)
    }
}

/// A user's current MFA status.
#[derive(Debug, Clone)]
pub struct MfaStatus {
    pub totp_enabled: bool,
    pub remaining_recovery_codes: u32,
}

/// Report a user's MFA status.
pub struct GetMfaStatus {
    pub user_id: AccountId,
}

impl Processor<GetMfaStatus> for MfaService {
    type Output = MfaStatus;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:GetMfaStatus")]
    async fn process(&self, input: GetMfaStatus) -> Result<Self::Output, Self::Error> {
        let totp_enabled = self
            .db
            .process(FindUserTotpByUserId {
                user_id: input.user_id.0,
            })
            .await?
            .is_some();
        let remaining = self
            .db
            .process(CountRecoveryCodesByUserId {
                user_id: input.user_id.0,
            })
            .await?;
        Ok(MfaStatus {
            totp_enabled,
            remaining_recovery_codes: remaining.max(0) as u32,
        })
    }
}

/// Outcome of [`VerifyMfaLogin`].
#[derive(Debug, Clone)]
pub enum VerifyMfaLoginResult {
    Success(SessionId),
    /// The MFA-login token is unknown or expired.
    InvalidToken,
    /// The submitted second-factor code was wrong.
    InvalidCode,
}

/// Exchange an MFA-login token + second factor for a session.
pub struct VerifyMfaLogin {
    pub mfa_token: [u8; 32],
    pub code: String,
}

impl Processor<VerifyMfaLogin> for MfaService {
    type Output = VerifyMfaLoginResult;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:VerifyMfaLogin")]
    async fn process(&self, input: VerifyMfaLogin) -> Result<Self::Output, Self::Error> {
        let Some(token) =
            MfaLoginToken::read(&mut self.redis.clone(), mfa_login_token_key(&input.mfa_token))
                .await?
        else {
            return Ok(VerifyMfaLoginResult::InvalidToken);
        };

        let Some(totp) = self
            .db
            .process(FindUserTotpByUserId {
                user_id: token.user_id,
            })
            .await?
        else {
            return Ok(VerifyMfaLoginResult::InvalidCode);
        };

        let ok = verify_totp(&totp.secret, &input.code)
            || self
                .db
                .process(ConsumeRecoveryCode {
                    user_id: token.user_id,
                    code_hash: hash_recovery_code(&input.code),
                })
                .await?;
        if !ok {
            // Leave the token so a mistyped code can be retried within its TTL.
            return Ok(VerifyMfaLoginResult::InvalidCode);
        }

        // Consume the token only once the second factor is confirmed.
        MfaLoginToken::delete(&mut self.redis.clone(), mfa_login_token_key(&input.mfa_token))
            .await?;

        let session_id = self
            .session
            .process(CreateSession {
                user_id: AccountId(token.user_id),
                ip: token.ip.clone(),
                user_agent: token.user_agent.clone(),
                security_option: security_from_u8(token.security_option),
            })
            .await?;

        UserLoginEvent {
            user_id: token.user_id,
            ip: token.ip,
            user_agent: token.user_agent,
            at: to_unix(now_primitive()),
        }
        .send(&self.mq)
        .await?;

        Ok(VerifyMfaLoginResult::Success(session_id))
    }
}
