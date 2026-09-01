use crate::entities::db::account::AccountId;
use kanau::processor::Processor;
use sqlx::postgres::types::PgInterval;
use time::PrimitiveDateTime;
use wakuwaku::sqlx::DatabaseProcessor;

/// Opaque session identifier (a transparent wrapper over `String`).
#[derive(Clone, PartialEq, Eq, sqlx::Type)]
#[sqlx(transparent)]
pub struct SessionId(pub String);

impl core::fmt::Debug for SessionId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("SessionId")
            .field(&format!(
                "{}***",
                &self.0.chars().take(4).collect::<String>()
            ))
            .finish()
    }
}

/// A row of `auth.session`.
#[derive(Clone)]
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

/// Session hardening policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "session_security_option", rename_all = "snake_case")]
pub enum SessionSecurityOption {
    RejectDifferentIp,
    RejectDifferentIpOrUserAgent,
    None,
}

/// Insert a new session row.
pub struct CreateSession {
    pub session: SessionEntity,
}

impl Processor<CreateSession> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:CreateSession")]
    async fn process(&self, input: CreateSession) -> Result<Self::Output, Self::Error> {
        let s = input.session;
        sqlx::query!(
            r#"INSERT INTO auth.session
               (session_id, user_id, user_agent, ip_address, created_at, last_refreshed_at, lifespan, security_option, expired_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
            s.session_id.0,
            s.user.0,
            s.user_agent,
            s.ip_address,
            s.created_at,
            s.last_refreshed_at,
            s.lifespan,
            s.security_option as SessionSecurityOption,
            s.expired_at
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// Look up a session by id.
pub struct FindSessionById {
    pub session_id: SessionId,
}

impl Processor<FindSessionById> for DatabaseProcessor {
    type Output = Option<SessionEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:FindSessionById")]
    async fn process(&self, input: FindSessionById) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            SessionEntity,
            r#"SELECT session_id AS "session_id: SessionId", user_id AS "user: AccountId",
                      user_agent, ip_address, created_at, last_refreshed_at,
                      lifespan AS "lifespan: PgInterval",
                      security_option AS "security_option: SessionSecurityOption", expired_at
               FROM auth.session WHERE session_id = $1"#,
            input.session_id.0
        )
        .fetch_optional(self.db())
        .await
    }
}

/// Update the sliding-refresh timestamp of a session.
pub struct TouchSession {
    pub session_id: SessionId,
    pub last_refreshed_at: PrimitiveDateTime,
}

impl Processor<TouchSession> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:TouchSession")]
    async fn process(&self, input: TouchSession) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"UPDATE auth.session SET last_refreshed_at = $2 WHERE session_id = $1"#,
            input.session_id.0,
            input.last_refreshed_at
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// Delete a single session.
pub struct DeleteSession {
    pub session_id: SessionId,
}

impl Processor<DeleteSession> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:DeleteSession")]
    async fn process(&self, input: DeleteSession) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"DELETE FROM auth.session WHERE session_id = $1"#,
            input.session_id.0
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// List all sessions belonging to a user.
pub struct ListSessionsByUser {
    pub user_id: AccountId,
}

impl Processor<ListSessionsByUser> for DatabaseProcessor {
    type Output = Vec<SessionEntity>;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:ListSessionsByUser")]
    async fn process(&self, input: ListSessionsByUser) -> Result<Self::Output, Self::Error> {
        sqlx::query_as!(
            SessionEntity,
            r#"SELECT session_id AS "session_id: SessionId", user_id AS "user: AccountId",
                      user_agent, ip_address, created_at, last_refreshed_at,
                      lifespan AS "lifespan: PgInterval",
                      security_option AS "security_option: SessionSecurityOption", expired_at
               FROM auth.session WHERE user_id = $1 ORDER BY last_refreshed_at DESC"#,
            input.user_id.0
        )
        .fetch_all(self.db())
        .await
    }
}

/// Delete every session belonging to a user.
pub struct DeleteSessionsByUser {
    pub user_id: AccountId,
}

impl Processor<DeleteSessionsByUser> for DatabaseProcessor {
    type Output = ();
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:DeleteSessionsByUser")]
    async fn process(&self, input: DeleteSessionsByUser) -> Result<Self::Output, Self::Error> {
        sqlx::query!(
            r#"DELETE FROM auth.session WHERE user_id = $1"#,
            input.user_id.0
        )
        .execute(self.db())
        .await?;
        Ok(())
    }
}

/// Delete sessions past their absolute expiry or sliding lifespan.
pub struct DeleteExpiredSessions {
    pub now: PrimitiveDateTime,
}

impl Processor<DeleteExpiredSessions> for DatabaseProcessor {
    type Output = u64;
    type Error = sqlx::Error;
    #[tracing::instrument(skip_all, err, name = "SQL:DeleteExpiredSessions")]
    async fn process(&self, input: DeleteExpiredSessions) -> Result<Self::Output, Self::Error> {
        let result = sqlx::query!(
            r#"DELETE FROM auth.session
               WHERE (expired_at IS NOT NULL AND expired_at < $1)
                  OR (last_refreshed_at + lifespan < $1)"#,
            input.now
        )
        .execute(self.db())
        .await?;
        Ok(result.rows_affected())
    }
}
