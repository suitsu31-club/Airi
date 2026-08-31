use crate::entities::db::account::AccountId;
use time::PrimitiveDateTime;
use uuid::Uuid;

#[derive(Clone, sqlx::FromRow, PartialEq, Eq)]
pub struct ApiKey(pub String);

pub struct UserApiKeyEntity {
    pub id: Uuid,
    pub user: AccountId,
    pub key: ApiKey,
    pub remark: String,
    pub created_at: PrimitiveDateTime,
    pub valid_until: Option<PrimitiveDateTime>,
    pub scopes: Vec<String>,
}
