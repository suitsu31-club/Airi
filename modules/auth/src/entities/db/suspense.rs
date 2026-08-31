use crate::entities::db::account::AccountId;
use time::PrimitiveDateTime;

/// Represent account suspense/active change history. If the newest status is `Suspended`, the account is suspended.
pub struct AccountSuspenseEntity {
    pub id: i64,
    pub account_id: AccountId,
    pub status: SuspenseStatus,
    pub created_at: PrimitiveDateTime,
    pub reason: String,
    pub operated_by: Option<AccountId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
pub enum SuspenseStatus {
    Active,
    Suspended,
}
