//! Account registration and password management.

use crate::entities::db::account::{
    AccountId, CreateAccount, CreateAccountResult, FindAccountById, UpdatePasswordHash,
};
use crate::entities::db::credit::CreateCreditRow;
use crate::entities::db::invite::{
    AcceptPendingInvitationsByInvite, FindInviteByToken, FindPendingInvitationByInvite,
    InviteStatus, PendingInvitationStatus, SetInviteStatus,
};
use crate::entities::db::membership::CreateMembership;
use crate::utils::datetime::now_primitive;
use crate::utils::password::{Argon2PasswordAlgorithm, PasswordAlgorithm};
use base::events::{InvitationAcceptedEvent, UserRegisteredEvent};
use kanau::processor::Processor;
use uuid::Uuid;
use validator::ValidateEmail;
use wakuwaku::amqp::{AmqpMessageSend, AmqpPool};
use wakuwaku::redis::RedisConnection;
use wakuwaku::sqlx::DatabaseProcessor;

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
}

impl Processor<Register> for AccountService {
    type Output = RegisterResult;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:Register")]
    async fn process(&self, input: Register) -> Result<Self::Output, Self::Error> {
        if !input.email.validate_email() {
            return Err(wakuwaku::Error::InvalidInput);
        }
        let now = now_primitive();

        // Registration is invite-only: require a usable, email-bound Pending invite.
        let Some(token) = input.invite_token.as_ref() else {
            return Ok(RegisterResult::InvalidInvite);
        };
        let invite = self
            .db
            .process(FindInviteByToken {
                invite_token: token.clone(),
            })
            .await?
            .filter(|i| {
                matches!(i.status, InviteStatus::Pending)
                    && i.will_expire_at.is_none_or(|e| e > now)
            });
        let Some(invite) = invite else {
            return Ok(RegisterResult::InvalidInvite);
        };

        // The invite must be pinned to exactly the registrant's email.
        let pinned = self
            .db
            .process(FindPendingInvitationByInvite { invite: invite.id })
            .await?;
        let email_matches = pinned.as_ref().is_some_and(|p| {
            matches!(p.status, PendingInvitationStatus::Pending) && p.email == input.email
        });
        if !email_matches {
            return Ok(RegisterResult::InvalidInvite);
        }

        let password_hash = self
            .alg
            .hash_password(&input.password)
            .map_err(|e| wakuwaku::Error::BusinessPanic(e.into()))?;

        let new_member_username = input.username.clone();

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
                invited_by: Some(invite.id),
            })
            .await?;
        self.db
            .process(CreateCreditRow { account: user_id })
            .await?;

        self.db
            .process(SetInviteStatus {
                id: invite.id,
                status: InviteStatus::Accepted,
            })
            .await?;
        self.db
            .process(AcceptPendingInvitationsByInvite { invite: invite.id })
            .await?;

        InvitationAcceptedEvent {
            inviter_id: invite.owner.0,
            new_member_id: user_id.0,
            new_member_username,
            accepted_at: crate::utils::datetime::to_unix(now),
        }
        .send(&self.mq)
        .await?;

        UserRegisteredEvent {
            user_id: user_id.0,
            email: input.email,
            invited_by: Some(invite.id.0),
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
    #[tracing::instrument(skip_all, err, name = "Service:ChangePassword")]
    async fn process(&self, input: ChangePassword) -> Result<Self::Output, Self::Error> {
        let account = self
            .db
            .process(FindAccountById { id: input.user_id })
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
