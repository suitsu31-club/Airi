//! Ephemeral pending-TOTP-enrollment secret, keyed by `totp_setup:{user_id}`.
//!
//! Written when a user starts TOTP enrollment and read back when they confirm
//! the first code. It carries a short TTL so an abandoned enrollment expires
//! rather than lingering. The secret is never logged.

use kanau::{RkyvMessageDe, RkyvMessageSer};
use uuid::Uuid;
use wakuwaku::redis::{KeyValue, KeyValueRead, KeyValueWrite, RedisKey};

/// The candidate TOTP secret for an in-progress enrollment.
#[derive(Clone, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, RkyvMessageSer, RkyvMessageDe)]
pub struct PendingTotpSetup {
    pub user_id: Uuid,
    pub secret: Box<[u8]>,
}

impl core::fmt::Debug for PendingTotpSetup {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PendingTotpSetup")
            .field("user_id", &self.user_id)
            .field("secret", &"***")
            .finish()
    }
}

/// Build the Redis key for a user's pending enrollment.
pub fn pending_totp_key(user_id: Uuid) -> RedisKey {
    RedisKey::from(format!("totp_setup:{user_id}"))
}

impl KeyValue for PendingTotpSetup {
    type Key = RedisKey;
    type Value = Self;

    fn key(&self) -> Self::Key {
        pending_totp_key(self.user_id)
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

impl KeyValueRead for PendingTotpSetup {}
impl KeyValueWrite for PendingTotpSetup {}
