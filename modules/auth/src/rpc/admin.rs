//! gRPC adapter for `airi.admin.AdminManage`.

use crate::entities::db::account::AccountId;
use crate::entities::db::admin_view::AdminUserRow;
use crate::entities::db::membership::AdminRole;
use crate::rpc::middleware::UserId;
use crate::rpc::parse_uuid;
use crate::services::admin::{
    AdminService, BanUser, GetServerConfig, GetUser, GrantInvitations, InvalidateInvite,
    ListAuditLogs, ListUsers, SetServerConfig, SetUserRole, UnbanUser,
};
use crate::utils::datetime::to_unix;
use app_protobuf::admin::admin_manage_server::AdminManage;
use app_protobuf::{admin as pb, shared};
use kanau::processor::Processor;
use tonic::{Request, Response, Status};

/// Implements the admin `AdminManage` service.
#[derive(Clone)]
pub struct AdminRpc {
    pub admin: AdminService,
}

fn admin_user_info(row: AdminUserRow) -> pb::AdminUserInfo {
    pb::AdminUserInfo {
        user_id: row.id.0.to_string(),
        username: row.username,
        email: row.email,
        level: row.level.unwrap_or(0),
        admin_role: row.admin_role.map(|r| r.as_str().to_string()),
        suspended: row.suspended,
        registered_at: to_unix(row.registered_at),
    }
}

#[tonic::async_trait]
impl AdminManage for AdminRpc {
    async fn list_users(
        &self,
        request: Request<pb::ListUsersRequest>,
    ) -> Result<Response<pb::ListUsersReply>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let users = self
            .admin
            .process(ListUsers {
                actor: AccountId(user.0),
                limit: req.limit as i64,
                offset: req.offset as i64,
            })
            .await?;
        Ok(Response::new(pb::ListUsersReply {
            users: users.into_iter().map(admin_user_info).collect(),
        }))
    }

    async fn get_user(
        &self,
        request: Request<pb::GetUserRequest>,
    ) -> Result<Response<pb::GetUserReply>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let id = parse_uuid(&req.user_id)?;
        let row = self
            .admin
            .process(GetUser {
                actor: AccountId(user.0),
                id: AccountId(id),
            })
            .await?;
        Ok(Response::new(pb::GetUserReply {
            user: row.map(admin_user_info),
        }))
    }

    async fn ban_user(
        &self,
        request: Request<pb::BanUserRequest>,
    ) -> Result<Response<shared::Empty>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let target = parse_uuid(&req.user_id)?;
        self.admin
            .process(BanUser {
                actor: AccountId(user.0),
                target: AccountId(target),
                reason: req.reason,
            })
            .await?;
        Ok(Response::new(shared::Empty {}))
    }

    async fn unban_user(
        &self,
        request: Request<pb::UnbanUserRequest>,
    ) -> Result<Response<shared::Empty>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let target = parse_uuid(&req.user_id)?;
        self.admin
            .process(UnbanUser {
                actor: AccountId(user.0),
                target: AccountId(target),
            })
            .await?;
        Ok(Response::new(shared::Empty {}))
    }

    async fn set_user_role(
        &self,
        request: Request<pb::SetUserRoleRequest>,
    ) -> Result<Response<shared::Empty>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let target = parse_uuid(&req.user_id)?;
        let role = match req.role {
            Some(r) => {
                Some(AdminRole::parse(&r).ok_or_else(|| Status::invalid_argument("invalid role"))?)
            }
            None => None,
        };
        self.admin
            .process(SetUserRole {
                actor: AccountId(user.0),
                target: AccountId(target),
                role,
            })
            .await?;
        Ok(Response::new(shared::Empty {}))
    }

    async fn grant_invitations(
        &self,
        request: Request<pb::GrantInvitationsRequest>,
    ) -> Result<Response<pb::GrantInvitationsReply>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let target = parse_uuid(&req.user_id)?;
        let new_count = self
            .admin
            .process(GrantInvitations {
                actor: AccountId(user.0),
                target: AccountId(target),
                count: req.count,
                expire_in_secs: req.expire_in_secs,
            })
            .await? as i32;
        Ok(Response::new(pb::GrantInvitationsReply { new_count }))
    }

    async fn invalidate_invite(
        &self,
        request: Request<pb::InvalidateInviteRequest>,
    ) -> Result<Response<pb::InvalidateInviteReply>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let invalidated = self
            .admin
            .process(InvalidateInvite {
                actor: AccountId(user.0),
                invite_id: req.invite_id,
            })
            .await?;
        Ok(Response::new(pb::InvalidateInviteReply { invalidated }))
    }

    async fn get_server_config(
        &self,
        request: Request<pb::GetServerConfigRequest>,
    ) -> Result<Response<pb::GetServerConfigReply>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let value = self
            .admin
            .process(GetServerConfig {
                actor: AccountId(user.0),
                key: req.key,
            })
            .await?;
        Ok(Response::new(pb::GetServerConfigReply { value }))
    }

    async fn set_server_config(
        &self,
        request: Request<pb::SetServerConfigRequest>,
    ) -> Result<Response<shared::Empty>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        self.admin
            .process(SetServerConfig {
                actor: AccountId(user.0),
                key: req.key,
                value: req.value,
            })
            .await?;
        Ok(Response::new(shared::Empty {}))
    }

    async fn list_audit_logs(
        &self,
        request: Request<pb::ListAuditLogsRequest>,
    ) -> Result<Response<pb::ListAuditLogsReply>, Status> {
        let user = UserId::from_request(&request)?;
        let req = request.into_inner();
        let logs = self
            .admin
            .process(ListAuditLogs {
                actor: AccountId(user.0),
                limit: req.limit as i64,
                offset: req.offset as i64,
            })
            .await?;
        let entries = logs
            .into_iter()
            .map(|l| pb::AuditLogEntry {
                id: l.id,
                admin: l.admin.0.to_string(),
                operation_name: l.operation_name,
                operation_content: l.operation_content.to_string(),
                created_at: to_unix(l.created_at),
            })
            .collect();
        Ok(Response::new(pb::ListAuditLogsReply { entries }))
    }
}
