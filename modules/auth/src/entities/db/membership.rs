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
    pub available_invitation_count: i32,
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
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: FindMembershipByAccount) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            MembershipEntity,
            r#"SELECT account AS "account: AccountId", level,
                      admin_privilege AS "admin_privilege?: AdminRole",
                      invited_by AS "invited_by?: InviteId", available_invitation_count
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
    pub available_invitation_count: i32,
}

impl Processor<CreateMembership> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: CreateMembership) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"INSERT INTO auth.membership
               (account, level, admin_privilege, invited_by, available_invitation_count)
               VALUES ($1, $2, $3, $4, $5)"#,
            input.account.0,
            input.level,
            input.admin_privilege as Option<AdminRole>,
            input.invited_by.map(|i| i.0),
            input.available_invitation_count
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
    #[tracing::instrument(skip_all, err)]
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

/// Adjust the available invitation count by a delta, refusing to go negative.
/// Returns the new count, or `None` when the guard failed or the member is absent.
pub struct AdjustInvitationCount {
    pub account: AccountId,
    pub delta: i32,
}

impl Processor<AdjustInvitationCount> for DatabaseProcessor {
    type Output = Option<i32>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: AdjustInvitationCount) -> Result<Self::Output, Self::Error> {
        let row = sqlx::query!(
            r#"UPDATE auth.membership
               SET available_invitation_count = available_invitation_count + $2
               WHERE account = $1 AND available_invitation_count + $2 >= 0
               RETURNING available_invitation_count"#,
            input.account.0,
            input.delta
        )
        .fetch_optional(self.db())
        .await?;
        Ok(row.map(|r| r.available_invitation_count))
    }
}

/// Grant additional invitations to a member. Returns the new count.
pub struct GrantInvitations {
    pub account: AccountId,
    pub count: i32,
}

impl Processor<GrantInvitations> for DatabaseProcessor {
    type Output = Option<i32>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: GrantInvitations) -> Result<Self::Output, Self::Error> {
        let row = sqlx::query!(
            r#"UPDATE auth.membership
               SET available_invitation_count = available_invitation_count + $2
               WHERE account = $1
               RETURNING available_invitation_count"#,
            input.account.0,
            input.count
        )
        .fetch_optional(self.db())
        .await?;
        Ok(row.map(|r| r.available_invitation_count))
    }
}
