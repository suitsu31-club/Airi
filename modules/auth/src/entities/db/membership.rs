use crate::entities::db::account::AccountId;
use uuid::Uuid;

pub struct MembershipEntity {
    pub account: AccountId,
    pub level: i32,
    pub admin_privilege: Option<AdminRole>,
    pub invited_by: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(rename_all = "snake_case")]
pub enum AdminRole {
    /// The owner of the site. Can do anything
    SiteOwner,

    /// The server maintainer. Can change server settings, promote and demote `Assistant` roles,
    /// ban and unban users. But don't have community moderation privileges.
    Maintainer,

    /// The community moderator. Can moderate community content, ban and unban users, premote and
    /// demote `Assistant` roles. But can't change server settings.
    Moderator,

    /// The assistant that helps to moderate community content.
    Assistant,
}
