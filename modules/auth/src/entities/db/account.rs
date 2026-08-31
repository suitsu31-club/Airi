use time::PrimitiveDateTime;
use uuid::Uuid;

#[derive(Debug, Copy, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AccountId(pub Uuid);

pub struct AccountEntity {
    pub id: AccountId,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub registered_at: PrimitiveDateTime,
}
