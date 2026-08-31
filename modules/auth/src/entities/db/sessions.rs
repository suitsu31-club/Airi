use crate::entities::db::account::AccountId;
use sqlx::postgres::types::PgInterval;
use time::PrimitiveDateTime;

#[derive(Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct SessionId(pub String);

impl core::fmt::Debug for SessionId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("SessionId")
            .field(&format!(
                "{}***{}",
                &self.0.chars().take(4).collect::<String>(),
                &self
                    .0
                    .chars()
                    .take(self.0.len().saturating_sub(4))
                    .collect::<String>()
            ))
            .finish()
    }
}

pub struct SessionEntity {
    pub session_id: SessionId,
    pub user: AccountId,
    pub user_agent: String,
    pub ip_address: String,
    pub created_at: PrimitiveDateTime,
    pub last_refreshed_at: PrimitiveDateTime,
    pub lifespan: PgInterval,
    pub security_option: SessionSecurityOption,
    pub expired_at: Option<PrimitiveDateTime>,
}

pub enum SessionSecurityOption {
    RejectDifferentIp,
    RejectDifferentIpOrUserAgent,
    None,
}
