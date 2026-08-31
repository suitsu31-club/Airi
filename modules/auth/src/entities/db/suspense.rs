use crate::entities::db::account::AccountId;
use kanau::processor::Processor;
use time::PrimitiveDateTime;
use wakuwaku::sqlx::DatabaseProcessor;

/// A row of `auth.account_suspense`. The newest row's [`SuspenseStatus`]
/// determines whether the account is currently suspended.
pub struct AccountSuspenseEntity {
    pub id: i64,
    pub account_id: AccountId,
    pub status: SuspenseStatus,
    pub created_at: PrimitiveDateTime,
    pub reason: String,
    pub operated_by: Option<AccountId>,
}

/// Suspension state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "suspense_status", rename_all = "snake_case")]
pub enum SuspenseStatus {
    Active,
    Suspended,
}

/// Append a suspense/active history entry.
pub struct InsertSuspense {
    pub account_id: AccountId,
    pub status: SuspenseStatus,
    pub reason: String,
    pub operated_by: Option<AccountId>,
}

impl Processor<InsertSuspense> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: InsertSuspense) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"INSERT INTO auth.account_suspense (account_id, status, reason, operated_by)
               VALUES ($1, $2, $3, $4)"#,
            input.account_id.0,
            input.status as SuspenseStatus,
            input.reason,
            input.operated_by.map(|a| a.0)
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// Fetch the most recent suspense entry for an account.
pub struct FindLatestSuspense {
    pub account_id: AccountId,
}

impl Processor<FindLatestSuspense> for DatabaseProcessor {
    type Output = Option<AccountSuspenseEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: FindLatestSuspense) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            AccountSuspenseEntity,
            r#"SELECT id, account_id AS "account_id: AccountId",
                      status AS "status: SuspenseStatus", created_at, reason,
                      operated_by AS "operated_by?: AccountId"
               FROM auth.account_suspense
               WHERE account_id = $1 ORDER BY created_at DESC LIMIT 1"#,
            input.account_id.0
        )
        .fetch_optional(self.db())
        .await
    }
}

/// Return whether an account is currently suspended.
pub struct IsSuspended {
    pub account_id: AccountId,
}

impl Processor<IsSuspended> for DatabaseProcessor {
    type Output = bool;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: IsSuspended) -> Result<Self::Output, Self::Error> {
        let row = sqlx::query!(
            r#"SELECT status AS "status: SuspenseStatus"
               FROM auth.account_suspense
               WHERE account_id = $1 ORDER BY created_at DESC LIMIT 1"#,
            input.account_id.0
        )
        .fetch_optional(self.db())
        .await?;
        Ok(matches!(row.map(|r| r.status), Some(SuspenseStatus::Suspended)))
    }
}
