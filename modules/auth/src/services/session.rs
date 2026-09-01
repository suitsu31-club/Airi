//! Session lifecycle service.

use crate::config::AuthConfig;
use crate::entities::db::account::AccountId;
use crate::entities::db::sessions::{
    self as db_session, SessionEntity, SessionId, SessionSecurityOption,
};
use crate::entities::redis::session_cache::{SessionCache, security_to_u8, session_cache_key};
use crate::utils::datetime::{now_primitive, pg_interval_from_secs, to_unix};
use crate::utils::identity::{IdentityVerifier, SessionIdVerify};
use base::config_provider::find_config_from_redis;
use kanau::processor::Processor;
use rand::RngCore;
use std::time::Duration as StdDuration;
use time::Duration;
use wakuwaku::redis::{KeyValue, KeyValueWrite, RedisConnection};
use wakuwaku::sqlx::DatabaseProcessor;

/// Creates, refreshes, and terminates sessions, keeping the Redis cache in sync.
#[derive(Clone)]
pub struct SessionService {
    pub db: DatabaseProcessor,
    pub redis: RedisConnection,
}

fn generate_session_id() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    fast32::base32::RFC4648_NOPAD.encode(&bytes)
}

impl SessionService {
    fn verifier(&self) -> IdentityVerifier {
        IdentityVerifier {
            db: self.db.clone(),
            redis: self.redis.clone(),
        }
    }

    async fn write_cache(
        &self,
        session: &SessionEntity,
        ttl_secs: u64,
    ) -> Result<(), wakuwaku::Error> {
        let cache = SessionCache {
            session_id: session.session_id.0.clone(),
            user_id: session.user.0,
            ip_address: session.ip_address.clone(),
            user_agent: session.user_agent.clone(),
            security_option: security_to_u8(session.security_option),
            last_refreshed: to_unix(session.last_refreshed_at),
        };
        cache
            .write_with_ttl(&mut self.redis.clone(), StdDuration::from_secs(ttl_secs))
            .await
    }
}

/// Create a new session for an authenticated user.
pub struct CreateSession {
    pub user_id: AccountId,
    pub ip: String,
    pub user_agent: String,
    pub security_option: SessionSecurityOption,
}

impl Processor<CreateSession> for SessionService {
    type Output = SessionId;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:CreateSession")]
    async fn process(&self, input: CreateSession) -> Result<Self::Output, Self::Error> {
        let cfg = find_config_from_redis::<AuthConfig>(&mut self.redis.clone()).await?;
        let id = generate_session_id();
        let now = now_primitive();
        let entity = SessionEntity {
            session_id: SessionId(id.clone()),
            user: input.user_id,
            user_agent: input.user_agent,
            ip_address: input.ip,
            created_at: now,
            last_refreshed_at: now,
            lifespan: pg_interval_from_secs(cfg.session_refresh_ttl_secs),
            security_option: input.security_option,
            expired_at: Some(now + Duration::seconds(cfg.session_absolute_lifespan_secs as i64)),
        };
        // Persist the authoritative DB row first; only cache after it succeeds so
        // a failed insert can never leave a cache-only session the verifier trusts.
        self.db
            .process(db_session::CreateSession {
                session: entity.clone(),
            })
            .await?;
        self.write_cache(&entity, cfg.session_refresh_ttl_secs).await?;
        Ok(SessionId(id))
    }
}

/// Outcome of a session refresh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshResult {
    Refreshed,
    NotFound,
}

/// Refresh (slide) a session, validating the presented IP/user agent.
pub struct RefreshSession {
    pub session_id: SessionId,
    pub ip: String,
    pub user_agent: String,
}

impl Processor<RefreshSession> for SessionService {
    type Output = RefreshResult;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:RefreshSession")]
    async fn process(&self, input: RefreshSession) -> Result<Self::Output, Self::Error> {
        let verify = self
            .verifier()
            .process(SessionIdVerify {
                session_id: input.session_id.clone(),
                ip: input.ip,
                user_agent: input.user_agent,
            })
            .await;
        match verify {
            Ok(_) => {
                let now = now_primitive();
                self.db
                    .process(db_session::TouchSession {
                        session_id: input.session_id.clone(),
                        last_refreshed_at: now,
                    })
                    .await?;
                let cfg = find_config_from_redis::<AuthConfig>(&mut self.redis.clone()).await?;
                if let Some(mut session) = self
                    .db
                    .process(db_session::FindSessionById {
                        session_id: input.session_id,
                    })
                    .await?
                {
                    session.last_refreshed_at = now;
                    self.write_cache(&session, cfg.session_refresh_ttl_secs).await?;
                }
                Ok(RefreshResult::Refreshed)
            }
            Err(wakuwaku::Error::NotFound) => Ok(RefreshResult::NotFound),
            Err(e) => Err(e),
        }
    }
}

/// Terminate a single session.
pub struct TerminateSession {
    pub session_id: SessionId,
}

impl Processor<TerminateSession> for SessionService {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:TerminateSession")]
    async fn process(&self, input: TerminateSession) -> Result<Self::Output, Self::Error> {
        self.db
            .process(db_session::DeleteSession {
                session_id: input.session_id.clone(),
            })
            .await?;
        SessionCache::delete(
            &mut self.redis.clone(),
            session_cache_key(&input.session_id.0),
        )
        .await?;
        Ok(())
    }
}

/// Terminate every session belonging to a user.
pub struct TerminateAllSessions {
    pub user_id: AccountId,
}

impl Processor<TerminateAllSessions> for SessionService {
    type Output = ();
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:TerminateAllSessions")]
    async fn process(&self, input: TerminateAllSessions) -> Result<Self::Output, Self::Error> {
        let sessions = self
            .db
            .process(db_session::ListSessionsByUser {
                user_id: input.user_id,
            })
            .await?;
        self.db
            .process(db_session::DeleteSessionsByUser {
                user_id: input.user_id,
            })
            .await?;
        let mut conn = self.redis.clone();
        for session in sessions {
            SessionCache::delete(&mut conn, session_cache_key(&session.session_id.0)).await?;
        }
        Ok(())
    }
}

/// List a user's active sessions.
pub struct ListUserSessions {
    pub user_id: AccountId,
}

impl Processor<ListUserSessions> for SessionService {
    type Output = Vec<SessionEntity>;
    type Error = wakuwaku::Error;
    #[tracing::instrument(skip_all, err, name = "Service:ListUserSessions")]
    async fn process(&self, input: ListUserSessions) -> Result<Self::Output, Self::Error> {
        Ok(self
            .db
            .process(db_session::ListSessionsByUser {
                user_id: input.user_id,
            })
            .await?)
    }
}
