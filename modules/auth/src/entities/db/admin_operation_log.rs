use crate::entities::db::account::AccountId;
use time::PrimitiveDateTime;

/// Admin operation log for audit use. Only log management operations that follow RBAC, not including
/// ReBAC operations.
pub struct AdminOperationLogEntity {
    pub id: i64,
    pub admin: AccountId,
    pub operation_name: String,
    pub operation_content: serde_json::Value,
    pub created_at: PrimitiveDateTime,
}
