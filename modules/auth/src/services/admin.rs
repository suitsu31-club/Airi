//! Administrative management operations (RBAC-enforced, audited).

use crate::entities::db::account::AccountId;
use crate::entities::db::admin_operation_log::{
    self as db_audit, AddAuditLog, AdminOperationLogEntity,
};
use crate::entities::db::admin_view::{AdminUserRow, GetAdminUser, ListAdminUsers};
use crate::entities::db::invite::{
    CountFreeInvitesByOwner, CreateInvite, ExpirePendingInvitationsByInvite,
    InvalidateInvite as DbInvalidateInvite, InviteId, InviteStatus, generate_invite_token,
};
use crate::entities::db::membership::{AdminRole, FindMembershipByAccount, SetAdminRole};
use crate::entities::db::suspense::{InsertSuspense, SuspenseStatus};
use crate::services::session::{SessionService, TerminateAllSessions};
use crate::utils::datetime::now_primitive;
use crate::utils::rbac::AdminOperation;
use base::config_provider::{get_config_value, set_config_value};
use kanau::processor::Processor;
use serde_json::json;
use time::Duration;
use wakuwaku::redis::RedisConnection;
use wakuwaku::sqlx::DatabaseProcessor;

/// Roles permitted to read admin data.
const READ_ROLES: &[AdminRole] = &[
    AdminRole::SiteOwner,
    AdminRole::Maintainer,
    AdminRole::Moderator,
    AdminRole::Assistant,
];
/// Roles permitted to read/write server config and audit logs.
const CONFIG_ROLES: &[AdminRole] = &[AdminRole::SiteOwner, AdminRole::Maintainer];

/// Administrative operations. Each op resolves the caller's role, enforces RBAC,
/// records an audit entry for mutations, then acts.
#[derive(Clone)]
pub struct AdminService {
    pub db: DatabaseProcessor,
    pub redis: RedisConnection,
    pub session: SessionService,
}

impl AdminService {
    async fn role_of(&self, actor: AccountId) -> Result<AdminRole, wakuwaku::Error> {
        self.db
            .process(FindMembershipByAccount { account: actor })
            .await?
            .and_then(|m| m.admin_privilege)
            .ok_or(wakuwaku::Error::PermissionsDenied)
    }

    async fn audit(
        &self,
        admin: AccountId,
        name: &str,
        content: serde_json::Value,
    ) -> Result<(), wakuwaku::Error> {
        self.db
            .process(AddAuditLog {
                admin,
                operation_name: name.to_string(),
                operation_content: content,
            })
            .await?;
        Ok(())
    }
}

/// List users for the admin console.
pub struct ListUsers {
    pub actor: AccountId,
    pub limit: i64,
    pub offset: i64,
}

impl Processor<ListUsers> for AdminService {
    type Output = Vec<AdminUserRow>;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:ListUsers")]
    async fn process(&self, input: ListUsers) -> Result<Self::Output, Self::Error> {
        let role = self.role_of(input.actor).await?;
        if !READ_ROLES.contains(&role) {
            return Err(wakuwaku::Error::PermissionsDenied);
        }
        Ok(self
            .db
            .process(ListAdminUsers {
                limit: input.limit,
                offset: input.offset,
            })
            .await?)
    }
}

/// Fetch a single user for the admin console.
pub struct GetUser {
    pub actor: AccountId,
    pub id: AccountId,
}

impl Processor<GetUser> for AdminService {
    type Output = Option<AdminUserRow>;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:GetUser")]
    async fn process(&self, input: GetUser) -> Result<Self::Output, Self::Error> {
        let role = self.role_of(input.actor).await?;
        if !READ_ROLES.contains(&role) {
            return Err(wakuwaku::Error::PermissionsDenied);
        }
        Ok(self.db.process(GetAdminUser { id: input.id }).await?)
    }
}

/// Ban (suspend) a user and terminate their sessions.
pub struct BanUser {
    pub actor: AccountId,
    pub target: AccountId,
    pub reason: String,
}

impl AdminOperation for BanUser {
    const ALLOWED_ROLES: &'static [AdminRole] = &[
        AdminRole::SiteOwner,
        AdminRole::Maintainer,
        AdminRole::Moderator,
    ];
    const OPERATION_NAME: &'static str = "ban_user";
    fn audit_content(&self) -> serde_json::Value {
        json!({ "target": self.target.0, "reason": self.reason })
    }
}

impl Processor<BanUser> for AdminService {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:BanUser")]
    async fn process(&self, input: BanUser) -> Result<Self::Output, Self::Error> {
        let role = self.role_of(input.actor).await?;
        if !BanUser::is_allowed(role) {
            return Err(wakuwaku::Error::PermissionsDenied);
        }
        self.db
            .process(InsertSuspense {
                account_id: input.target,
                status: SuspenseStatus::Suspended,
                reason: input.reason.clone(),
                operated_by: Some(input.actor),
            })
            .await?;
        self.session
            .process(TerminateAllSessions {
                user_id: input.target,
            })
            .await?;
        self.audit(input.actor, BanUser::OPERATION_NAME, input.audit_content())
            .await?;
        Ok(())
    }
}

/// Unban (reactivate) a user.
pub struct UnbanUser {
    pub actor: AccountId,
    pub target: AccountId,
}

impl AdminOperation for UnbanUser {
    const ALLOWED_ROLES: &'static [AdminRole] = &[
        AdminRole::SiteOwner,
        AdminRole::Maintainer,
        AdminRole::Moderator,
    ];
    const OPERATION_NAME: &'static str = "unban_user";
    fn audit_content(&self) -> serde_json::Value {
        json!({ "target": self.target.0 })
    }
}

impl Processor<UnbanUser> for AdminService {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:UnbanUser")]
    async fn process(&self, input: UnbanUser) -> Result<Self::Output, Self::Error> {
        let role = self.role_of(input.actor).await?;
        if !UnbanUser::is_allowed(role) {
            return Err(wakuwaku::Error::PermissionsDenied);
        }
        self.db
            .process(InsertSuspense {
                account_id: input.target,
                status: SuspenseStatus::Active,
                reason: String::new(),
                operated_by: Some(input.actor),
            })
            .await?;
        self.audit(
            input.actor,
            UnbanUser::OPERATION_NAME,
            input.audit_content(),
        )
        .await?;
        Ok(())
    }
}

/// Set or clear a user's administrative role.
pub struct SetUserRole {
    pub actor: AccountId,
    pub target: AccountId,
    pub role: Option<AdminRole>,
}

impl AdminOperation for SetUserRole {
    const ALLOWED_ROLES: &'static [AdminRole] = &[
        AdminRole::SiteOwner,
        AdminRole::Maintainer,
        AdminRole::Moderator,
    ];
    const OPERATION_NAME: &'static str = "set_user_role";
    fn audit_content(&self) -> serde_json::Value {
        json!({ "target": self.target.0, "role": self.role.map(|r| r.as_str()) })
    }
}

impl Processor<SetUserRole> for AdminService {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:SetUserRole")]
    async fn process(&self, input: SetUserRole) -> Result<Self::Output, Self::Error> {
        let role = self.role_of(input.actor).await?;
        if !SetUserRole::is_allowed(role) {
            return Err(wakuwaku::Error::PermissionsDenied);
        }
        // Only the site owner may grant elevated roles.
        if let Some(target_role) = input.role {
            let elevated = matches!(
                target_role,
                AdminRole::SiteOwner | AdminRole::Maintainer | AdminRole::Moderator
            );
            if elevated && role != AdminRole::SiteOwner {
                return Err(wakuwaku::Error::PermissionsDenied);
            }
        }
        self.db
            .process(SetAdminRole {
                account: input.target,
                role: input.role,
            })
            .await?;
        self.audit(
            input.actor,
            SetUserRole::OPERATION_NAME,
            input.audit_content(),
        )
        .await?;
        Ok(())
    }
}

/// Grant a user invitation slots (`Free` invites), optionally with an expiry.
pub struct GrantInvitations {
    pub actor: AccountId,
    pub target: AccountId,
    pub count: i32,
    pub expire_in_secs: Option<u64>,
}

impl AdminOperation for GrantInvitations {
    const ALLOWED_ROLES: &'static [AdminRole] = &[AdminRole::SiteOwner, AdminRole::Moderator];
    const OPERATION_NAME: &'static str = "grant_invitations";
    fn audit_content(&self) -> serde_json::Value {
        json!({
            "target": self.target.0,
            "count": self.count,
            "expire_in_secs": self.expire_in_secs,
        })
    }
}

impl Processor<GrantInvitations> for AdminService {
    type Output = i64;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:GrantInvitations")]
    async fn process(&self, input: GrantInvitations) -> Result<Self::Output, Self::Error> {
        let role = self.role_of(input.actor).await?;
        if !GrantInvitations::is_allowed(role) {
            return Err(wakuwaku::Error::PermissionsDenied);
        }
        if input.count <= 0 {
            return Err(wakuwaku::Error::InvalidInput);
        }
        let now = now_primitive();
        let will_expire_at = input
            .expire_in_secs
            .map(|s| now + Duration::seconds(s as i64));
        for _ in 0..input.count {
            self.db
                .process(CreateInvite {
                    owner: input.target,
                    invite_token: generate_invite_token(),
                    status: InviteStatus::Free,
                    source: "admin_grant".to_string(),
                    will_expire_at,
                })
                .await?;
        }
        self.audit(
            input.actor,
            GrantInvitations::OPERATION_NAME,
            input.audit_content(),
        )
        .await?;
        self.db
            .process(CountFreeInvitesByOwner {
                owner: input.target,
                now,
            })
            .await
            .map_err(Into::into)
    }
}

/// Invalidate an invite slot, permitted only when it is `Free` or `Pending`.
pub struct InvalidateInvite {
    pub actor: AccountId,
    pub invite_id: i64,
}

impl AdminOperation for InvalidateInvite {
    const ALLOWED_ROLES: &'static [AdminRole] = &[AdminRole::SiteOwner, AdminRole::Moderator];
    const OPERATION_NAME: &'static str = "invalidate_invite";
    fn audit_content(&self) -> serde_json::Value {
        json!({ "invite_id": self.invite_id })
    }
}

impl Processor<InvalidateInvite> for AdminService {
    type Output = bool;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:InvalidateInvite")]
    async fn process(&self, input: InvalidateInvite) -> Result<Self::Output, Self::Error> {
        let role = self.role_of(input.actor).await?;
        if !InvalidateInvite::is_allowed(role) {
            return Err(wakuwaku::Error::PermissionsDenied);
        }
        let invite_id = InviteId(input.invite_id);
        let Some(prev) = self
            .db
            .process(DbInvalidateInvite { id: invite_id })
            .await?
        else {
            return Ok(false);
        };
        if matches!(prev, InviteStatus::Pending) {
            self.db
                .process(ExpirePendingInvitationsByInvite { invite: invite_id })
                .await?;
        }
        self.audit(
            input.actor,
            InvalidateInvite::OPERATION_NAME,
            input.audit_content(),
        )
        .await?;
        Ok(true)
    }
}

/// Read a server config value (JSON) by key.
pub struct GetServerConfig {
    pub actor: AccountId,
    pub key: String,
}

impl Processor<GetServerConfig> for AdminService {
    type Output = String;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:GetServerConfig")]
    async fn process(&self, input: GetServerConfig) -> Result<Self::Output, Self::Error> {
        let role = self.role_of(input.actor).await?;
        if !CONFIG_ROLES.contains(&role) {
            return Err(wakuwaku::Error::PermissionsDenied);
        }
        let value = get_config_value(self.db.db(), &input.key)
            .await?
            .unwrap_or(serde_json::Value::Null);
        serde_json::to_string(&value).map_err(|e| wakuwaku::Error::SerializeError(e.into()))
    }
}

/// Set a server config value (JSON) by key.
pub struct SetServerConfig {
    pub actor: AccountId,
    pub key: String,
    pub value: String,
}

impl AdminOperation for SetServerConfig {
    const ALLOWED_ROLES: &'static [AdminRole] = CONFIG_ROLES;
    const OPERATION_NAME: &'static str = "set_server_config";
    fn audit_content(&self) -> serde_json::Value {
        json!({ "key": self.key })
    }
}

impl Processor<SetServerConfig> for AdminService {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:SetServerConfig")]
    async fn process(&self, input: SetServerConfig) -> Result<Self::Output, Self::Error> {
        let role = self.role_of(input.actor).await?;
        if !SetServerConfig::is_allowed(role) {
            return Err(wakuwaku::Error::PermissionsDenied);
        }
        let value: serde_json::Value =
            serde_json::from_str(&input.value).map_err(|_| wakuwaku::Error::InvalidInput)?;
        set_config_value(self.db.db(), &mut self.redis.clone(), &input.key, &value).await?;
        self.audit(
            input.actor,
            SetServerConfig::OPERATION_NAME,
            input.audit_content(),
        )
        .await?;
        Ok(())
    }
}

/// List audit-log entries.
pub struct ListAuditLogs {
    pub actor: AccountId,
    pub limit: i64,
    pub offset: i64,
}

impl Processor<ListAuditLogs> for AdminService {
    type Output = Vec<AdminOperationLogEntity>;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:ListAuditLogs")]
    async fn process(&self, input: ListAuditLogs) -> Result<Self::Output, Self::Error> {
        let role = self.role_of(input.actor).await?;
        if !CONFIG_ROLES.contains(&role) {
            return Err(wakuwaku::Error::PermissionsDenied);
        }
        Ok(self
            .db
            .process(db_audit::ListAuditLogs {
                limit: input.limit,
                offset: input.offset,
            })
            .await?)
    }
}
