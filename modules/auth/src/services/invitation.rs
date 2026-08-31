//! Invitation minting and sending.

use crate::config::AuthConfig;
use crate::entities::db::account::AccountId;
use crate::entities::db::invite::{
    CreateInvite, CreatePendingInvitation, FindInviteById, FindPendingInvitation, InviteEntity,
    InviteStatus, ListInvitesByOwner, ListPendingInvitationsByOwner, PendingInvitationEntity,
    TouchPendingInvitation, generate_invite_token,
};
use crate::entities::db::membership::{AdjustInvitationCount, AdminRole, FindMembershipByAccount};
use crate::utils::datetime::{now_primitive, to_unix};
use base::config_provider::find_config_from_redis;
use base::events::InvitationSentEvent;
use kanau::processor::Processor;
use time::Duration;
use validator::ValidateEmail;
use wakuwaku::amqp::{AmqpMessageSend, AmqpPool};
use wakuwaku::redis::RedisConnection;
use wakuwaku::sqlx::DatabaseProcessor;

/// Mints and sends invitations.
#[derive(Clone)]
pub struct InvitationService {
    pub db: DatabaseProcessor,
    pub redis: RedisConnection,
    pub mq: AmqpPool,
}

/// Mint `count` free invites for an owner (admin action).
pub struct CreateInvitation {
    pub actor: AccountId,
    pub owner: AccountId,
    pub count: i32,
}

impl Processor<CreateInvitation> for InvitationService {
    type Output = Vec<String>;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: CreateInvitation) -> Result<Self::Output, Self::Error> {
        let role = self
            .db
            .process(FindMembershipByAccount { account: input.actor })
            .await?
            .and_then(|m| m.admin_privilege)
            .ok_or(wakuwaku::Error::PermissionsDenied)?;
        if !matches!(role, AdminRole::SiteOwner | AdminRole::Moderator) {
            return Err(wakuwaku::Error::PermissionsDenied);
        }
        if input.count <= 0 {
            return Err(wakuwaku::Error::InvalidInput);
        }
        let mut tokens = Vec::with_capacity(input.count as usize);
        for _ in 0..input.count {
            let token = generate_invite_token();
            self.db
                .process(CreateInvite {
                    owner: input.owner,
                    invite_token: token.clone(),
                    status: InviteStatus::Free,
                    source: "admin_grant".to_string(),
                    will_expire_at: None,
                })
                .await?;
            tokens.push(token);
        }
        Ok(tokens)
    }
}

/// Outcome of [`SendInvitation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendInvitationResult {
    Sent,
    NoInvitationLeft,
    EmailInvalid,
}

/// Send an invitation email, consuming one of the actor's invitation slots.
pub struct SendInvitation {
    pub actor: AccountId,
    pub email: String,
}

impl Processor<SendInvitation> for InvitationService {
    type Output = SendInvitationResult;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: SendInvitation) -> Result<Self::Output, Self::Error> {
        if !input.email.validate_email() {
            return Ok(SendInvitationResult::EmailInvalid);
        }
        if self
            .db
            .process(AdjustInvitationCount {
                account: input.actor,
                delta: -1,
            })
            .await?
            .is_none()
        {
            return Ok(SendInvitationResult::NoInvitationLeft);
        }

        let cfg = find_config_from_redis::<AuthConfig>(&mut self.redis.clone()).await?;
        let now = now_primitive();
        let token = generate_invite_token();
        let invite = self
            .db
            .process(CreateInvite {
                owner: input.actor,
                invite_token: token.clone(),
                status: InviteStatus::Pending,
                source: "user".to_string(),
                will_expire_at: Some(now + Duration::seconds(cfg.invitation_expiry_secs as i64)),
            })
            .await?;
        let pending = self
            .db
            .process(CreatePendingInvitation {
                invite: invite.id,
                email: input.email.clone(),
                will_release_at: now
                    + Duration::seconds(cfg.pending_invitation_release_secs as i64),
            })
            .await?;

        InvitationSentEvent {
            invite_id: pending.id,
            email: input.email,
            invite_token: token,
            sent_at: to_unix(now),
        }
        .send(&self.mq)
        .await?;

        Ok(SendInvitationResult::Sent)
    }
}

/// Outcome of [`ResendInvitationEmail`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResendResult {
    Sent,
    NotFound,
}

/// Resend an invitation email for one of the actor's pending invitations.
pub struct ResendInvitationEmail {
    pub actor: AccountId,
    pub pending_invitation_id: i64,
}

impl Processor<ResendInvitationEmail> for InvitationService {
    type Output = ResendResult;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: ResendInvitationEmail) -> Result<Self::Output, Self::Error> {
        let Some(pending) = self
            .db
            .process(FindPendingInvitation {
                id: input.pending_invitation_id,
            })
            .await?
        else {
            return Ok(ResendResult::NotFound);
        };
        let Some(invite) = self.db.process(FindInviteById { id: pending.invite }).await? else {
            return Ok(ResendResult::NotFound);
        };
        if invite.owner != input.actor {
            return Ok(ResendResult::NotFound);
        }

        let cfg = find_config_from_redis::<AuthConfig>(&mut self.redis.clone()).await?;
        let now = now_primitive();
        self.db
            .process(TouchPendingInvitation {
                id: pending.id,
                will_release_at: now
                    + Duration::seconds(cfg.pending_invitation_release_secs as i64),
            })
            .await?;

        InvitationSentEvent {
            invite_id: pending.id,
            email: pending.email,
            invite_token: invite.invite_token,
            sent_at: to_unix(now),
        }
        .send(&self.mq)
        .await?;

        Ok(ResendResult::Sent)
    }
}

/// A user's invitations and their pending sends.
pub struct MyInvitations {
    pub invites: Vec<InviteEntity>,
    pub pending: Vec<PendingInvitationEntity>,
}

/// List the actor's invitations and pending sends.
pub struct ListMyInvitations {
    pub actor: AccountId,
}

impl Processor<ListMyInvitations> for InvitationService {
    type Output = MyInvitations;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: ListMyInvitations) -> Result<Self::Output, Self::Error> {
        let invites = self
            .db
            .process(ListInvitesByOwner { owner: input.actor })
            .await?;
        let pending = self
            .db
            .process(ListPendingInvitationsByOwner { owner: input.actor })
            .await?;
        Ok(MyInvitations { invites, pending })
    }
}
