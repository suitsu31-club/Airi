//! gRPC adapter for `airi.internal.Identity` (inter-service, no auth layer).

use crate::entities::db::account::AccountId;
use crate::entities::db::api_key::ApiKey;
use crate::entities::db::sessions::SessionId;
use crate::rpc::parse_uuid;
use crate::services::profile::{GetMyProfile, ProfileData, ProfileService};
use crate::utils::datetime::to_unix;
use crate::utils::identity::{ApiKeyVerify, IdentityVerifier, SessionIdVerify};
use app_protobuf::internal as pb;
use app_protobuf::internal::identity_server::Identity;
use kanau::processor::Processor;
use tonic::{Request, Response, Status};

/// Implements the internal `Identity` service.
#[derive(Clone)]
pub struct IdentityRpc {
    pub verifier: IdentityVerifier,
    pub profile: ProfileService,
}

fn user_identity(data: ProfileData) -> pb::UserIdentity {
    let level = data.membership.as_ref().map_or(0, |m| m.level);
    pb::UserIdentity {
        user_id: data.account.id.0.to_string(),
        username: data.account.username,
        email: data.account.email,
        avatar_url: data.account.avatar_url,
        level,
    }
}

#[tonic::async_trait]
impl Identity for IdentityRpc {
    async fn check_session(
        &self,
        request: Request<pb::CheckSessionRequest>,
    ) -> Result<Response<pb::CheckSessionReply>, Status> {
        let req = request.into_inner();
        match self
            .verifier
            .process(SessionIdVerify {
                session_id: SessionId(req.session_id),
                ip: req.ip,
                user_agent: req.user_agent,
            })
            .await
        {
            Ok(v) => Ok(Response::new(pb::CheckSessionReply {
                valid: true,
                user_id: Some(v.user.0.to_string()),
            })),
            Err(wakuwaku::Error::NotFound | wakuwaku::Error::PermissionsDenied) => {
                Ok(Response::new(pb::CheckSessionReply {
                    valid: false,
                    user_id: None,
                }))
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn check_api_key(
        &self,
        request: Request<pb::CheckApiKeyRequest>,
    ) -> Result<Response<pb::CheckApiKeyReply>, Status> {
        let req = request.into_inner();
        match self
            .verifier
            .process(ApiKeyVerify {
                api_key: ApiKey(req.api_key),
            })
            .await
        {
            Ok(v) => Ok(Response::new(pb::CheckApiKeyReply {
                valid: true,
                user_id: Some(v.user.0.to_string()),
                valid_until: v.valid_until.map(to_unix).unwrap_or(0),
                scopes: v.scopes,
            })),
            Err(wakuwaku::Error::NotFound | wakuwaku::Error::PermissionsDenied) => {
                Ok(Response::new(pb::CheckApiKeyReply {
                    valid: false,
                    user_id: None,
                    valid_until: 0,
                    scopes: Vec::new(),
                }))
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn get_user_by_id(
        &self,
        request: Request<pb::GetUserByIdRequest>,
    ) -> Result<Response<pb::GetUserByIdReply>, Status> {
        let req = request.into_inner();
        let id = parse_uuid(&req.user_id)?;
        match self
            .profile
            .process(GetMyProfile {
                user_id: AccountId(id),
            })
            .await
        {
            Ok(data) => Ok(Response::new(pb::GetUserByIdReply {
                user: Some(user_identity(data)),
            })),
            Err(wakuwaku::Error::NotFound) => {
                Ok(Response::new(pb::GetUserByIdReply { user: None }))
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn get_users_by_ids(
        &self,
        request: Request<pb::GetUsersByIdsRequest>,
    ) -> Result<Response<pb::GetUsersByIdsReply>, Status> {
        let req = request.into_inner();
        let mut users = Vec::new();
        for raw in req.user_ids {
            let Ok(id) = uuid::Uuid::parse_str(&raw) else {
                continue;
            };
            if let Ok(data) = self
                .profile
                .process(GetMyProfile {
                    user_id: AccountId(id),
                })
                .await
            {
                users.push(user_identity(data));
            }
        }
        Ok(Response::new(pb::GetUsersByIdsReply { users }))
    }
}
