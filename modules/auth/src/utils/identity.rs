use crate::entities::db::account::AccountId;
use crate::entities::db::api_key::{ApiKey, FindApiKeyByHash, hash_api_key};
use crate::entities::db::sessions::{
    FindSessionById, SessionEntity, SessionId, SessionSecurityOption,
};
use crate::entities::redis::session_cache::{
    SessionCache, security_from_u8, security_to_u8, session_cache_key,
};
use crate::utils::datetime::{now_primitive, to_unix};
use kanau::processor::Processor;
use time::PrimitiveDateTime;
use wakuwaku::redis::{KeyValueRead, KeyValueWrite, RedisConnection};
use wakuwaku::sqlx::DatabaseProcessor;

/// Verifies opaque credentials (session ids and API keys) against the database,
/// accelerated by a Redis session cache.
#[derive(Clone)]
pub struct IdentityVerifier {
    pub db: DatabaseProcessor,
    pub redis: RedisConnection,
}

/// Request to verify a session id, hardened by the origin IP/user agent.
pub struct SessionIdVerify {
    pub session_id: SessionId,
    pub ip: String,
    pub user_agent: String,
}

/// A successfully verified session.
pub struct VerifiedSession {
    pub user: AccountId,
    pub session_id: SessionId,
}

impl Processor<SessionIdVerify> for IdentityVerifier {
    type Error = wakuwaku::Error;
    type Output = VerifiedSession;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: SessionIdVerify) -> Result<Self::Output, Self::Error> {
        let mut conn = self.redis.clone();
        let key = session_cache_key(&input.session_id.0);

        // Fast path: a cache hit implies the session is unexpired, because the
        // cache TTL is set to the session's remaining lifespan on population.
        if let Some(cache) = SessionCache::read(&mut conn, key).await? {
            enforce_security(
                security_from_u8(cache.security_option),
                &cache.ip_address,
                &cache.user_agent,
                &input.ip,
                &input.user_agent,
            )?;
            return Ok(VerifiedSession {
                user: AccountId(cache.user_id),
                session_id: input.session_id,
            });
        }

        // Slow path: authoritative lookup from Postgres.
        let session = self
            .db
            .process(FindSessionById {
                session_id: input.session_id.clone(),
            })
            .await?
            .ok_or(wakuwaku::Error::NotFound)?;

        let now = now_primitive();
        let expiry = sliding_expiry(&session);
        if expiry <= now {
            return Err(wakuwaku::Error::NotFound);
        }
        if let Some(absolute) = session.expired_at
            && absolute <= now
        {
            return Err(wakuwaku::Error::NotFound);
        }

        enforce_security(
            session.security_option,
            &session.ip_address,
            &session.user_agent,
            &input.ip,
            &input.user_agent,
        )?;

        // Repopulate the cache with a TTL equal to the remaining lifespan.
        let ttl_secs = (expiry - now).whole_seconds().max(0) as u64;
        let cache = SessionCache {
            session_id: session.session_id.0.clone(),
            user_id: session.user.0,
            ip_address: session.ip_address.clone(),
            user_agent: session.user_agent.clone(),
            security_option: security_to_u8(session.security_option),
            last_refreshed: to_unix(session.last_refreshed_at),
        };
        cache
            .write_with_ttl(&mut conn, std::time::Duration::from_secs(ttl_secs))
            .await?;

        Ok(VerifiedSession {
            user: session.user,
            session_id: session.session_id,
        })
    }
}

/// Request to verify an API key.
pub struct ApiKeyVerify {
    pub api_key: ApiKey,
}

/// A successfully verified API key.
pub struct ApiKeyVerifyOutput {
    pub user: AccountId,
    pub valid_until: Option<PrimitiveDateTime>,
    pub scopes: Vec<String>,
}

impl Processor<ApiKeyVerify> for IdentityVerifier {
    type Error = wakuwaku::Error;
    type Output = ApiKeyVerifyOutput;
    #[tracing::instrument(skip_all, err)]
    async fn process(&self, input: ApiKeyVerify) -> Result<Self::Output, Self::Error> {
        let key_hash = hash_api_key(&input.api_key.0);
        let key = self
            .db
            .process(FindApiKeyByHash { key_hash })
            .await?
            .ok_or(wakuwaku::Error::NotFound)?;

        if let Some(valid_until) = key.valid_until
            && valid_until <= now_primitive()
        {
            return Err(wakuwaku::Error::NotFound);
        }

        Ok(ApiKeyVerifyOutput {
            user: key.user,
            valid_until: key.valid_until,
            scopes: key.scopes,
        })
    }
}

/// Enforce a session's security option against the presented IP/user agent.
fn enforce_security(
    option: SessionSecurityOption,
    stored_ip: &str,
    stored_ua: &str,
    req_ip: &str,
    req_ua: &str,
) -> Result<(), wakuwaku::Error> {
    let ok = match option {
        SessionSecurityOption::RejectDifferentIp => stored_ip == req_ip,
        SessionSecurityOption::RejectDifferentIpOrUserAgent => {
            stored_ip == req_ip && stored_ua == req_ua
        }
        SessionSecurityOption::None => true,
    };
    if ok {
        Ok(())
    } else {
        Err(wakuwaku::Error::PermissionsDenied)
    }
}

/// Compute a session's sliding expiry (`last_refreshed_at + lifespan`).
fn sliding_expiry(session: &SessionEntity) -> PrimitiveDateTime {
    let interval = &session.lifespan;
    let seconds = interval.months as i64 * 30 * 86_400
        + interval.days as i64 * 86_400
        + interval.microseconds / 1_000_000;
    session.last_refreshed_at + time::Duration::seconds(seconds)
}
