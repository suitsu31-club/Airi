use crate::entities::db::sessions::SessionSecurityOption;
use kanau::{RkyvMessageDe, RkyvMessageSer};
use uuid::Uuid;
use wakuwaku::redis::{KeyValue, KeyValueRead, KeyValueWrite, RedisKey};

/// Redis verification cache for a session, keyed by `session:{session_id}`.
///
/// A cache hit lets the identity verifier validate a session without a database
/// round-trip; a miss falls back to Postgres and repopulates the cache.
#[derive(
    Debug, Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, RkyvMessageSer, RkyvMessageDe,
)]
pub struct SessionCache {
    pub session_id: String,
    pub user_id: Uuid,
    pub ip_address: String,
    pub user_agent: String,
    /// Encoded [`SessionSecurityOption`] discriminant (see [`security_from_u8`]).
    pub security_option: u8,
    /// Last sliding-refresh timestamp (unix seconds).
    pub last_refreshed: u64,
}

/// Build the Redis key for a session id.
pub fn session_cache_key(session_id: &str) -> RedisKey {
    RedisKey::from(format!("session:{session_id}"))
}

/// Encode a [`SessionSecurityOption`] to its cached discriminant.
pub fn security_to_u8(option: SessionSecurityOption) -> u8 {
    match option {
        SessionSecurityOption::RejectDifferentIp => 0,
        SessionSecurityOption::RejectDifferentIpOrUserAgent => 1,
        SessionSecurityOption::None => 2,
    }
}

/// Decode a cached discriminant back into a [`SessionSecurityOption`].
pub fn security_from_u8(value: u8) -> SessionSecurityOption {
    match value {
        0 => SessionSecurityOption::RejectDifferentIp,
        1 => SessionSecurityOption::RejectDifferentIpOrUserAgent,
        _ => SessionSecurityOption::None,
    }
}

impl KeyValue for SessionCache {
    type Key = RedisKey;
    type Value = Self;

    fn key(&self) -> Self::Key {
        session_cache_key(&self.session_id)
    }
    fn value(&self) -> Self::Value {
        self.clone()
    }
    fn into_value(self) -> Self::Value {
        self
    }
    fn new(_key: Self::Key, value: Self::Value) -> Self {
        value
    }
}

impl KeyValueRead for SessionCache {}
impl KeyValueWrite for SessionCache {}
