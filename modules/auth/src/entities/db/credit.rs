use crate::entities::db::account::AccountId;
use kanau::processor::Processor;
use rust_decimal::Decimal;
use time::PrimitiveDateTime;
use wakuwaku::sqlx::DatabaseProcessor;

/// A row of `auth.credit`. `total_amount` is the full balance; `frozen_amount`
/// is the portion held. Available balance is `total_amount - frozen_amount`.
pub struct CreditEntity {
    pub account: AccountId,
    pub total_amount: Decimal,
    pub frozen_amount: Decimal,
}

/// A row of `auth.credit_change_history`.
pub struct CreditChangeHistoryEntity {
    pub id: i64,
    pub account: AccountId,
    pub available_amount_change: Decimal,
    pub frozen_amount_change: Decimal,
    pub reason: String,
    pub created_at: PrimitiveDateTime,
}

/// Look up a credit row by account.
pub struct FindCreditByAccount {
    pub account: AccountId,
}

impl Processor<FindCreditByAccount> for DatabaseProcessor {
    type Output = Option<CreditEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:FindCreditByAccount")]
    async fn process(&self, input: FindCreditByAccount) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            CreditEntity,
            r#"SELECT account AS "account: AccountId", total_amount, frozen_amount
               FROM auth.credit WHERE account = $1"#,
            input.account.0
        )
        .fetch_optional(self.db())
        .await
    }
}

/// Create a zeroed credit row for a new account.
pub struct CreateCreditRow {
    pub account: AccountId,
}

impl Processor<CreateCreditRow> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:CreateCreditRow")]
    async fn process(&self, input: CreateCreditRow) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"INSERT INTO auth.credit (account) VALUES ($1)
               ON CONFLICT (account) DO NOTHING"#,
            input.account.0
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// Apply a credit delta atomically and append a history record.
///
/// `frozen_amount` moves by `frozen_delta`; `total_amount` moves by
/// `available_delta + frozen_delta`, so the available balance
/// (`total - frozen`) moves by `available_delta`.
pub struct ApplyCreditChange {
    pub account: AccountId,
    pub available_delta: Decimal,
    pub frozen_delta: Decimal,
    pub reason: String,
}

impl Processor<ApplyCreditChange> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL-Transaction:ApplyCreditChange")]
    async fn process(&self, input: ApplyCreditChange) -> Result<Self::Output, Self::Error> {
        let mut tx = self.db().begin().await?;
        sqlx::query!(
            r#"INSERT INTO auth.credit (account, total_amount, frozen_amount)
               VALUES ($1, $2::numeric + $3::numeric, $3::numeric)
               ON CONFLICT (account) DO UPDATE
               SET total_amount = auth.credit.total_amount + EXCLUDED.total_amount,
                   frozen_amount = auth.credit.frozen_amount + EXCLUDED.frozen_amount"#,
            input.account.0,
            input.available_delta,
            input.frozen_delta
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            r#"INSERT INTO auth.credit_change_history
               (account, available_amount_change, frozen_amount_change, reason)
               VALUES ($1, $2, $3, $4)"#,
            input.account.0,
            input.available_delta,
            input.frozen_delta,
            input.reason
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

/// List a member's credit change history, newest first.
pub struct ListCreditHistory {
    pub account: AccountId,
    pub limit: i64,
    pub offset: i64,
}

impl Processor<ListCreditHistory> for DatabaseProcessor {
    type Output = Vec<CreditChangeHistoryEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:ListCreditHistory")]
    async fn process(&self, input: ListCreditHistory) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            CreditChangeHistoryEntity,
            r#"SELECT id, account AS "account: AccountId", available_amount_change,
                      frozen_amount_change, reason, created_at
               FROM auth.credit_change_history
               WHERE account = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"#,
            input.account.0,
            input.limit,
            input.offset
        )
        .fetch_all(self.db())
        .await
    }
}
