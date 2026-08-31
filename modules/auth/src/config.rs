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
    /// Whether open (invite-less) registration is permitted.
    pub registration_open: bool,
    /// Invitations granted to a newly registered member.
    pub default_invitation_count: i32,
    /// How long a minted invite remains valid before expiring.
    pub invitation_expiry_secs: u64,
    /// How long a pending (sent) invitation is held before it is released and
    /// the sender's invitation count refunded.
    pub pending_invitation_release_secs: u64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            session_refresh_ttl_secs: 604_800,
            session_absolute_lifespan_secs: 2_592_000,
            registration_open: false,
            default_invitation_count: 0,
            invitation_expiry_secs: 1_209_600,
            pending_invitation_release_secs: 259_200,
        }
    }
}

impl ConfigJson for AuthConfig {
    const KEY: &'static str = "auth";
}
