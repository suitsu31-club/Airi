//! gRPC adapter for `airi.auth.UserProfile`.

use crate::entities::db::account::AccountId;
use crate::rpc::middleware::UserId;
use crate::rpc::parse_uuid;
use crate::services::api_key::{ApiKeyService, CreateApiKey, ListApiKeys, RevokeApiKey};
use crate::services::profile::{
    GetMyCredit, GetMyCreditLog, GetMyInvitationSummary, GetMyProfile, GetPublicProfile,
    ProfileData, ProfileService,
};
use crate::utils::datetime::{from_unix, to_unix};
use app_protobuf::auth as pb;
use app_protobuf::auth::user_profile_server::UserProfile;
use kanau::processor::Processor;
use tonic::{Request, Response, Status};

/// Implements the public `UserProfile` service.
#[derive(Clone)]
pub struct UserProfileRpc {
    pub profile: ProfileService,
    pub api_key: ApiKeyService,
}

fn my_profile(data: ProfileData) -> pb::MyProfile {
    let level = data.membership.as_ref().map_or(0, |m| m.level);
    let admin_role = data
        .membership
        .as_ref()
        .and_then(|m| m.admin_privilege)
        .map(|r| r.as_str().to_string());
    pb::MyProfile {
        user_id: data.account.id.0.to_string(),
        username: data.account.username,
        email: data.account.email,
        avatar_url: data.account.avatar_url,
        level,
        admin_role,
        registered_at: to_unix(data.account.registered_at),
    }
}

#[tonic::async_trait]
impl UserProfile for UserProfileRpc {
    async fn get_my_profile(
        &self,
        request: Request<pb::GetMyProfileRequest>,
    ) -> Result<Response<pb::MyProfile>, Status> {
        let user = UserId::from_request(&request)?;
        let data = self
            .profile
            .process(GetMyProfile {
                user_id: AccountId(user.0),
            })
            .await?;
        Ok(Response::new(my_profile(data)))
    }

    async fn get_public_profile(
        &self,
        request: Request<pb::GetPublicProfileRequest>,
    ) -> Result<Response<pb::PublicProfile>, Status> {
        let req = request.into_inner();
        let user_id = parse_uuid(&req.user_id)?;
        let data = self
            .profile
            .process(GetPublicProfile {
                user_id: AccountId(user_id),
            })
            .await?;
        let level = data.membership.as_ref().map_or(0, |m| m.level);
        Ok(Response::new(pb::PublicProfile {
            user_id: data.account.id.0.to_string(),
            username: data.account.username,
            avatar_url: data.account.avatar_url,
            level,
            registered_at: to_unix(data.account.registered_at),
        }))
    }

    async fn get_my_credit(
        &self,
        request: Request<pb::GetMyCreditRequest>,
    ) -> Result<Response<pb::CreditInfo>, Status> {
        let user = UserId::from_request(&request)?;
        let credit = self
            .profile
            .process(GetMyCredit {
                user_id: AccountId(user.0),
            })
            .await?;
        let available = credit.total_amount - credit.frozen_amount;
        Ok(Response::new(pb::CreditInfo {
            total_amount: credit.total_amount.to_string(),
            frozen_amount: credit.frozen_amount.to_string(),
            available_amount: available.to_string(),
        }))
    }

    async fn get_my_credit_log(
        &self,
        request: Request<pb::GetMyCreditLogRequest>,
    ) -> Result<Response<pb::GetMyCreditLogReply>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let entries = self
            .profile
            .process(GetMyCreditLog {
                user_id: AccountId(user.0),
                limit: req.limit as i64,
                offset: req.offset as i64,
            })
            .await?;
        let entries = entries
            .into_iter()
            .map(|e| pb::CreditLogEntry {
                id: e.id,
                available_amount_change: e.available_amount_change.to_string(),
                frozen_amount_change: e.frozen_amount_change.to_string(),
                reason: e.reason,
                created_at: to_unix(e.created_at),
            })
            .collect();
        Ok(Response::new(pb::GetMyCreditLogReply { entries }))
    }

    async fn get_my_invitations(
        &self,
        request: Request<pb::GetMyInvitationsRequest>,
    ) -> Result<Response<pb::GetMyInvitationsReply>, Status> {
        let user = UserId::from_request(&request)?;
        let summary = self
            .profile
            .process(GetMyInvitationSummary {
                user_id: AccountId(user.0),
            })
            .await?;
        Ok(Response::new(pb::GetMyInvitationsReply {
            available_count: summary.available_count,
            sent_count: summary.sent_count,
        }))
    }

    async fn create_api_key(
        &self,
        request: Request<pb::CreateApiKeyRequest>,
    ) -> Result<Response<pb::CreateApiKeyReply>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let created = self
            .api_key
            .process(CreateApiKey {
                user_id: AccountId(user.0),
                remark: req.remark,
                valid_until: req.valid_until.map(from_unix),
                scopes: req.scopes,
            })
            .await?;
        Ok(Response::new(pb::CreateApiKeyReply {
            id: created.id.to_string(),
            api_key: created.plaintext,
        }))
    }

    async fn list_api_keys(
        &self,
        request: Request<pb::ListApiKeysRequest>,
    ) -> Result<Response<pb::ListApiKeysReply>, Status> {
        let user = UserId::from_request(&request)?;
        let keys = self
            .api_key
            .process(ListApiKeys {
                user_id: AccountId(user.0),
            })
            .await?;
        let keys = keys
            .into_iter()
            .map(|k| pb::ApiKeyInfo {
                id: k.id.to_string(),
                remark: k.remark,
                created_at: to_unix(k.created_at),
                valid_until: k.valid_until.map(to_unix),
                scopes: k.scopes,
            })
            .collect();
        Ok(Response::new(pb::ListApiKeysReply { keys }))
    }

    async fn revoke_api_key(
        &self,
        request: Request<pb::RevokeApiKeyRequest>,
    ) -> Result<Response<pb::RevokeApiKeyReply>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let id = parse_uuid(&req.id)?;
        let revoked = self
            .api_key
            .process(RevokeApiKey {
                user_id: AccountId(user.0),
                id,
            })
            .await?;
        Ok(Response::new(pb::RevokeApiKeyReply { revoked }))
    }
}
