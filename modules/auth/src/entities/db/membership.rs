use crate::entities::db::account::AccountId;
use crate::entities::db::invite::InviteId;
use kanau::processor::Processor;
use wakuwaku::sqlx::DatabaseProcessor;

/// A row of `auth.membership`.
pub struct MembershipEntity {
    pub account: AccountId,
    pub level: i32,
    pub admin_privilege: Option<AdminRole>,
    pub invited_by: Option<InviteId>,
}

/// Administrative role granted to a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "admin_role", rename_all = "snake_case")]
pub enum AdminRole {
    /// The owner of the site. Can do anything.
    SiteOwner,

    /// The server maintainer. Can change server settings, promote and demote
    /// `Assistant` roles, ban and unban users. But doesn't have community
    /// moderation privileges.
    Maintainer,

    /// The community moderator. Can moderate community content, ban and unban
    /// users, promote and demote `Assistant` roles. But can't change server
    /// settings.
    Moderator,

    /// The assistant that helps to moderate community content.
    Assistant,
}

impl AdminRole {
    /// Snake-case wire representation of the role.
    pub fn as_str(&self) -> &'static str {
        match self {
            AdminRole::SiteOwner => "site_owner",
            AdminRole::Maintainer => "maintainer",
            AdminRole::Moderator => "moderator",
            AdminRole::Assistant => "assistant",
        }
    }

    /// Parse a role from its snake-case wire representation.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "site_owner" => Some(AdminRole::SiteOwner),
            "maintainer" => Some(AdminRole::Maintainer),
            "moderator" => Some(AdminRole::Moderator),
            "assistant" => Some(AdminRole::Assistant),
            _ => None,
        }
    }
}

/// Look up a member row by account id.
pub struct FindMembershipByAccount {
    pub account: AccountId,
}

impl Processor<FindMembershipByAccount> for DatabaseProcessor {
    type Output = Option<MembershipEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:FindMembershipByAccount")]
    async fn process(&self, input: FindMembershipByAccount) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            MembershipEntity,
            r#"SELECT account AS "account: AccountId", level,
                      admin_privilege AS "admin_privilege?: AdminRole",
                      invited_by AS "invited_by?: InviteId"
               FROM auth.membership WHERE account = $1"#,
            input.account.0
        )
        .fetch_optional(self.db())
        .await
    }
}

/// Create a member row.
pub struct CreateMembership {
    pub account: AccountId,
    pub level: i32,
    pub admin_privilege: Option<AdminRole>,
    pub invited_by: Option<InviteId>,
}

impl Processor<CreateMembership> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:CreateMembership")]
    async fn process(&self, input: CreateMembership) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"INSERT INTO auth.membership
               (account, level, admin_privilege, invited_by)
               VALUES ($1, $2, $3, $4)"#,
            input.account.0,
            input.level,
            input.admin_privilege as Option<AdminRole>,
            input.invited_by.map(|i| i.0)
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// Set (or clear) a member's administrative role.
pub struct SetAdminRole {
    pub account: AccountId,
    pub role: Option<AdminRole>,
}

impl Processor<SetAdminRole> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:SetAdminRole")]
    async fn process(&self, input: SetAdminRole) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"UPDATE auth.membership SET admin_privilege = $2 WHERE account = $1"#,
            input.account.0,
            input.role as Option<AdminRole>
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}
