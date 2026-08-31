use crate::entities::db::account::AccountId;
use kanau::processor::Processor;
use time::PrimitiveDateTime;
use wakuwaku::sqlx::DatabaseProcessor;

/// A row of `auth.admin_operation_log`. Only RBAC-governed management
/// operations are logged (not ReBAC operations).
pub struct AdminOperationLogEntity {
    pub id: i64,
    pub admin: AccountId,
    pub operation_name: String,
    pub operation_content: serde_json::Value,
    pub created_at: PrimitiveDateTime,
}

/// Append an audit-log entry.
pub struct AddAuditLog {
    pub admin: AccountId,
    pub operation_name: String,
    pub operation_content: serde_json::Value,
}

impl Processor<AddAuditLog> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: AddAuditLog) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"INSERT INTO auth.admin_operation_log (admin, operation_name, operation_content)
               VALUES ($1, $2, $3)"#,
            input.admin.0,
            input.operation_name,
            input.operation_content
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// List audit-log entries, newest first.
pub struct ListAuditLogs {
    pub limit: i64,
    pub offset: i64,
}

impl Processor<ListAuditLogs> for DatabaseProcessor {
    type Output = Vec<AdminOperationLogEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: ListAuditLogs) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            AdminOperationLogEntity,
            r#"SELECT id, admin AS "admin: AccountId", operation_name, operation_content,
                      created_at
               FROM auth.admin_operation_log ORDER BY created_at DESC LIMIT $1 OFFSET $2"#,
            input.limit,
            input.offset
        )
        .fetch_all(self.db())
        .await
    }
}
