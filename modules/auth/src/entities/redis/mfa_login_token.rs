//! Short-lived MFA-login token, keyed by `mfa_login:{base32(token)}`.
//!
//! Issued by the login flow when an account has TOTP enabled: instead of a
//! session, the caller receives an opaque token that pins the login context
//! (user, ip, user agent, requested session security). Exchanging it for a
//! session requires a valid TOTP or recovery code (see `VerifyMfaLogin`). The
//! token bytes are never logged.

use kanau::{RkyvMessageDe, RkyvMessageSer};
use uuid::Uuid;
use wakuwaku::redis::{KeyValue, KeyValueRead, KeyValueWrite, RedisKey};

/// TTL of an MFA-login token, in seconds.
pub const MFA_LOGIN_TOKEN_TTL_SECS: u64 = 300;

/// A pending MFA login awaiting second-factor verification.
#[derive(Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, RkyvMessageSer, RkyvMessageDe)]
pub struct MfaLoginToken {
    pub token: [u8; 32],
    pub user_id: Uuid,
    pub ip: String,
    pub user_agent: String,
    /// Encoded [`crate::entities::db::sessions::SessionSecurityOption`]
    /// discriminant (see `session_cache::security_from_u8`).
    pub security_option: u8,
}

impl core::fmt::Debug for MfaLoginToken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MfaLoginToken")
            .field("token", &"***")
            .field("user_id", &self.user_id)
            .field("ip", &self.ip)
            .field("user_agent", &self.user_agent)
            .field("security_option", &self.security_option)
            .finish()
    }
}

/// Build the Redis key for an MFA-login token.
pub fn mfa_login_token_key(token: &[u8; 32]) -> RedisKey {
    RedisKey::from(format!(
        "mfa_login:{}",
        fast32::base32::RFC4648_NOPAD.encode(token)
    ))
}

impl KeyValue for MfaLoginToken {
    type Key = RedisKey;
    type Value = Self;

    fn key(&self) -> Self::Key {
        mfa_login_token_key(&self.token)
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

impl KeyValueRead for MfaLoginToken {}
impl KeyValueWrite for MfaLoginToken {}
