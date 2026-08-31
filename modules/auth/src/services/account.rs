//! Account registration and password management.

use crate::config::AuthConfig;
use crate::entities::db::account::{
    AccountId, CreateAccount, CreateAccountResult, FindAccountById, UpdatePasswordHash,
};
use crate::entities::db::credit::CreateCreditRow;
use crate::entities::db::invite::{
    AcceptPendingInvitationsByInvite, FindInviteByToken, InviteStatus, SetInviteStatus,
};
use crate::entities::db::membership::CreateMembership;
use crate::utils::datetime::now_primitive;
use crate::utils::password::{Argon2PasswordAlgorithm, PasswordAlgorithm};
use base::config_provider::find_config_from_redis;
use base::events::UserRegisteredEvent;
use kanau::processor::Processor;
use uuid::Uuid;
use wakuwaku::amqp::{AmqpMessageSend, AmqpPool};
use wakuwaku::redis::RedisConnection;
use wakuwaku::sqlx::DatabaseProcessor;
use validator::ValidateEmail;

/// Registration and password operations.
#[derive(Clone)]
pub struct AccountService {
    pub db: DatabaseProcessor,
    pub redis: RedisConnection,
    pub mq: AmqpPool,
    pub alg: Argon2PasswordAlgorithm,
}

/// Register a new account.
pub struct Register {
    pub username: String,
    pub email: String,
    pub password: String,
    pub invite_token: Option<String>,
}

/// Outcome of [`Register`].
#[derive(Debug, Clone)]
pub enum RegisterResult {
    Success { user_id: AccountId },
    EmailTaken,
    UsernameTaken,
    InvalidInvite,
    RegistrationClosed,
}

impl Processor<Register> for AccountService {
    type Output = RegisterResult;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: Register) -> Result<Self::Output, Self::Error> {
        if !input.email.validate_email() {
            return Err(wakuwaku::Error::InvalidInput);
        }
        let cfg = find_config_from_redis::<AuthConfig>(&mut self.redis.clone()).await?;
        let now = now_primitive();

        let invite = match &input.invite_token {
            Some(token) => {
                self.db
                    .process(FindInviteByToken {
                        invite_token: token.clone(),
                    })
                    .await?
            }
            None => None,
        };
        let invite = invite.filter(|i| {
            matches!(i.status, InviteStatus::Pending | InviteStatus::Free)
                && i.will_expire_at.is_none_or(|e| e > now)
        });

        if !cfg.registration_open {
            if input.invite_token.is_none() {
                return Ok(RegisterResult::RegistrationClosed);
            }
            if invite.is_none() {
                return Ok(RegisterResult::InvalidInvite);
            }
        }

        let password_hash = self
            .alg
            .hash_password(&input.password)
            .map_err(|e| wakuwaku::Error::BusinessPanic(e.into()))?;

        let user_id = AccountId(Uuid::new_v4());
        match self
            .db
            .process(CreateAccount {
                id: user_id,
                username: input.username,
                email: input.email.clone(),
                avatar_url: None,
                password_hash,
            })
            .await?
        {
            CreateAccountResult::EmailTaken => return Ok(RegisterResult::EmailTaken),
            CreateAccountResult::UsernameTaken => return Ok(RegisterResult::UsernameTaken),
            CreateAccountResult::Success => {}
        }

        self.db
            .process(CreateMembership {
                account: user_id,
                level: 0,
                admin_privilege: None,
                invited_by: invite.as_ref().map(|i| i.id),
                available_invitation_count: cfg.default_invitation_count,
            })
            .await?;
        self.db.process(CreateCreditRow { account: user_id }).await?;

        if let Some(inv) = &invite {
            self.db
                .process(SetInviteStatus {
                    id: inv.id,
                    status: InviteStatus::Accepted,
                })
                .await?;
            self.db
                .process(AcceptPendingInvitationsByInvite { invite: inv.id })
                .await?;
        }

        UserRegisteredEvent {
            user_id: user_id.0,
            email: input.email,
            invited_by: invite.as_ref().map(|i| i.id.0),
            registered_at: crate::utils::datetime::to_unix(now),
        }
        .send(&self.mq)
        .await?;

        Ok(RegisterResult::Success { user_id })
    }
}

/// Change an account's password after verifying the old one.
pub struct ChangePassword {
    pub user_id: AccountId,
    pub old_password: String,
    pub new_password: String,
}

/// Outcome of [`ChangePassword`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangePasswordResult {
    Success,
    WrongPassword,
}

impl Processor<ChangePassword> for AccountService {
    type Output = ChangePasswordResult;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: ChangePassword) -> Result<Self::Output, Self::Error> {
        let account = self
            .db
            .process(FindAccountById {
                id: input.user_id,
            })
            .await?
            .ok_or(wakuwaku::Error::NotFound)?;

        if !self
            .alg
            .verify_password(&input.old_password, &account.password_hash)
        {
            return Ok(ChangePasswordResult::WrongPassword);
        }

        let password_hash = self
            .alg
            .hash_password(&input.new_password)
            .map_err(|e| wakuwaku::Error::BusinessPanic(e.into()))?;
        self.db
            .process(UpdatePasswordHash {
                id: input.user_id,
                password_hash,
            })
            .await?;
        Ok(ChangePasswordResult::Success)
    }
}
