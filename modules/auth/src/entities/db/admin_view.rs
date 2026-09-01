//! Read models for the admin console (account joined with membership/suspense).

use crate::entities::db::account::AccountId;
use crate::entities::db::membership::AdminRole;
use kanau::processor::Processor;
use time::PrimitiveDateTime;
use wakuwaku::sqlx::DatabaseProcessor;

/// A combined admin-facing view of a user.
pub struct AdminUserRow {
    pub id: AccountId,
    pub username: String,
    pub email: String,
    pub level: Option<i32>,
    pub admin_role: Option<AdminRole>,
    pub suspended: bool,
    pub registered_at: PrimitiveDateTime,
}

/// List users for the admin console, newest first.
pub struct ListAdminUsers {
    pub limit: i64,
    pub offset: i64,
}

impl Processor<ListAdminUsers> for DatabaseProcessor {
    type Output = Vec<AdminUserRow>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:ListAdminUsers")]
    async fn process(&self, input: ListAdminUsers) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            AdminUserRow,
            r#"SELECT a.id AS "id: AccountId", a.username, a.email, a.registered_at,
                      m.level AS "level?", m.admin_privilege AS "admin_role?: AdminRole",
                      COALESCE(
                          (SELECT s.status = 'suspended'
                           FROM auth.account_suspense s
                           WHERE s.account_id = a.id
                           ORDER BY s.created_at DESC LIMIT 1),
                          false
                      ) AS "suspended!"
               FROM auth.account a
               LEFT JOIN auth.membership m ON m.account = a.id
               ORDER BY a.registered_at DESC LIMIT $1 OFFSET $2"#,
            input.limit,
            input.offset
        )
        .fetch_all(self.db())
        .await
    }
}

/// Fetch a single admin-facing user view.
pub struct GetAdminUser {
    pub id: AccountId,
}

impl Processor<GetAdminUser> for DatabaseProcessor {
    type Output = Option<AdminUserRow>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:GetAdminUser")]
    async fn process(&self, input: GetAdminUser) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            AdminUserRow,
            r#"SELECT a.id AS "id: AccountId", a.username, a.email, a.registered_at,
                      m.level AS "level?", m.admin_privilege AS "admin_role?: AdminRole",
                      COALESCE(
                          (SELECT s.status = 'suspended'
                           FROM auth.account_suspense s
                           WHERE s.account_id = a.id
                           ORDER BY s.created_at DESC LIMIT 1),
                          false
                      ) AS "suspended!"
               FROM auth.account a
               LEFT JOIN auth.membership m ON m.account = a.id
               WHERE a.id = $1"#,
            input.id.0
        )
        .fetch_optional(self.db())
        .await
    }
}
