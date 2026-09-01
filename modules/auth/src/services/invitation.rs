//! Invitation minting and sending.

use crate::config::AuthConfig;
use crate::entities::db::account::{AccountId, FindAccountByEmail};
use crate::entities::db::invite::{
    ClaimFreeInviteAndPin, FindInviteById, FindPendingInvitation, InviteEntity, InviteStatus,
    ListInvitesByOwner, ListPendingInvitationsByOwner, PendingInvitationEntity,
    RefreshPendingInvite, TouchPendingInvitation, generate_invite_token,
};
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

/// Outcome of [`SendInvitation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendInvitationResult {
    Sent,
    NoInvitationLeft,
    EmailInvalid,
    /// The email already belongs to a registered account.
    AlreadyRegistered,
}

/// Send an invitation email, consuming one of the actor's invitation slots.
pub struct SendInvitation {
    pub actor: AccountId,
    pub email: String,
}

impl Processor<SendInvitation> for InvitationService {
    type Output = SendInvitationResult;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:SendInvitation")]
    async fn process(&self, input: SendInvitation) -> Result<Self::Output, Self::Error> {
        if !input.email.validate_email() {
            return Ok(SendInvitationResult::EmailInvalid);
        }

        // A registered account can never accept an invitation, so refuse to
        // send one (and never consume a slot) for an already-registered email.
        if self
            .db
            .process(FindAccountByEmail {
                email: input.email.clone(),
            })
            .await?
            .is_some()
        {
            return Ok(SendInvitationResult::AlreadyRegistered);
        }

        let cfg = find_config_from_redis::<AuthConfig>(&mut self.redis.clone()).await?;
        let now = now_primitive();
        let token = generate_invite_token();
        let expiry = now + Duration::seconds(cfg.invitation_expiry_secs as i64);

        // Atomically consume one usable Free slot and pin it to the recipient,
        // regenerating the token as the slot turns Pending.
        let Some(claimed) = self
            .db
            .process(ClaimFreeInviteAndPin {
                owner: input.actor,
                new_token: token.clone(),
                email: input.email.clone(),
                expiry,
                now,
            })
            .await?
        else {
            return Ok(SendInvitationResult::NoInvitationLeft);
        };

        InvitationSentEvent {
            invite_id: claimed.pending_id,
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
    #[tracing::instrument(skip_all, err, name = "Service:ResendInvitationEmail")]
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
        let Some(invite) = self
            .db
            .process(FindInviteById { id: pending.invite })
            .await?
        else {
            return Ok(ResendResult::NotFound);
        };
        // Only the owner may resend, and only while the slot is still Pending.
        if invite.owner != input.actor || !matches!(invite.status, InviteStatus::Pending) {
            return Ok(ResendResult::NotFound);
        }

        let cfg = find_config_from_redis::<AuthConfig>(&mut self.redis.clone()).await?;
        let now = now_primitive();
        let expiry = now + Duration::seconds(cfg.invitation_expiry_secs as i64);
        let token = generate_invite_token();

        // Regenerate the token (killing the previous link) and extend the expiry.
        self.db
            .process(RefreshPendingInvite {
                invite: invite.id,
                new_token: token.clone(),
                will_expire_at: expiry,
            })
            .await?;
        self.db
            .process(TouchPendingInvitation {
                id: pending.id,
                will_release_at: expiry,
            })
            .await?;

        InvitationSentEvent {
            invite_id: pending.id,
            email: pending.email,
            invite_token: token,
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
    #[tracing::instrument(skip_all, err, name = "Service:ListMyInvitations")]
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
