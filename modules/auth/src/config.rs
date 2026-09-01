//! Typed configuration for the `auth` module.

use base::config::ConfigJson;
use serde::{Deserialize, Serialize};

/// Runtime-tunable settings for authentication, sessions, and invitations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Sliding session window in seconds: a session expires if not refreshed
    /// within this period.
    pub session_refresh_ttl_secs: u64,
    /// Absolute session lifespan in seconds (hard cap regardless of refresh).
    pub session_absolute_lifespan_secs: u64,
    /// How long a sent (pending) invitation stays valid before it expires and
    /// its slot returns to the sender as `Free`.
    pub invitation_expiry_secs: u64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            session_refresh_ttl_secs: 604_800,
            session_absolute_lifespan_secs: 2_592_000,
            invitation_expiry_secs: 1_209_600,
        }
    }
}

impl ConfigJson for AuthConfig {
    const KEY: &'static str = "auth";
}
