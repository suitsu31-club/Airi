//! gRPC adapter for `airi.auth.Invitation`.

use crate::entities::db::account::AccountId;
use crate::rpc::middleware::UserId;
use crate::rpc::parse_uuid;
use crate::services::invitation::{
    CreateInvitation, InvitationService, ListMyInvitations, ResendInvitationEmail, ResendResult,
    SendInvitation, SendInvitationResult,
};
use crate::utils::datetime::to_unix;
use app_protobuf::auth as pb;
use app_protobuf::auth::invitation_server::Invitation;
use kanau::processor::Processor;
use tonic::{Request, Response, Status};

/// Implements the public `Invitation` service.
#[derive(Clone)]
pub struct InvitationRpc {
    pub invitation: InvitationService,
}

#[tonic::async_trait]
impl Invitation for InvitationRpc {
    async fn create_invitation(
        &self,
        request: Request<pb::CreateInvitationRequest>,
    ) -> Result<Response<pb::CreateInvitationReply>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let owner = match req.owner {
            Some(o) => AccountId(parse_uuid(&o)?),
            None => AccountId(user.0),
        };
        let invite_tokens = self
            .invitation
            .process(CreateInvitation {
                actor: AccountId(user.0),
                owner,
                count: req.count,
            })
            .await?;
        Ok(Response::new(pb::CreateInvitationReply { invite_tokens }))
    }

    async fn send_invitation(
        &self,
        request: Request<pb::SendInvitationRequest>,
    ) -> Result<Response<pb::SendInvitationReply>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let result = self
            .invitation
            .process(SendInvitation {
                actor: AccountId(user.0),
                email: req.email,
            })
            .await?;
        let code = match result {
            SendInvitationResult::Sent => pb::SendInvitationResult::Sent,
            SendInvitationResult::NoInvitationLeft => pb::SendInvitationResult::NoInvitationLeft,
            SendInvitationResult::EmailInvalid => pb::SendInvitationResult::EmailInvalid,
        };
        Ok(Response::new(pb::SendInvitationReply {
            result: code as i32,
        }))
    }

    async fn resend_invitation_email(
        &self,
        request: Request<pb::ResendInvitationEmailRequest>,
    ) -> Result<Response<pb::ResendInvitationEmailReply>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let result = self
            .invitation
            .process(ResendInvitationEmail {
                actor: AccountId(user.0),
                pending_invitation_id: req.pending_invitation_id,
            })
            .await?;
        let code = match result {
            ResendResult::Sent => pb::ResendResult::Sent,
            ResendResult::NotFound => pb::ResendResult::NotFound,
        };
        Ok(Response::new(pb::ResendInvitationEmailReply {
            result: code as i32,
        }))
    }

    async fn list_my_invitations(
        &self,
        request: Request<pb::ListMyInvitationsRequest>,
    ) -> Result<Response<pb::ListMyInvitationsReply>, Status> {
        let user = UserId::from_request(&request)?;
        let my = self
            .invitation
            .process(ListMyInvitations {
                actor: AccountId(user.0),
            })
            .await?;
        let invites = my
            .invites
            .into_iter()
            .map(|i| pb::InviteInfo {
                id: i.id.0,
                invite_token: i.invite_token,
                status: i.status.as_str().to_string(),
                created_at: to_unix(i.created_at),
                will_expire_at: i.will_expire_at.map(to_unix),
                source: i.source,
            })
            .collect();
        let pending = my
            .pending
            .into_iter()
            .map(|p| pb::PendingInvitationInfo {
                id: p.id,
                invite: p.invite.0,
                email: p.email,
                sent_at: to_unix(p.sent_at),
                will_release_at: to_unix(p.will_release_at),
                status: p.status.as_str().to_string(),
            })
            .collect();
        Ok(Response::new(pb::ListMyInvitationsReply { invites, pending }))
    }
}
