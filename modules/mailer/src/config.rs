//! Module configuration, stored in `base.module_config` and read through
//! [`ConfigLoader`](base::services::config::ConfigLoader).

use base::ModuleConfig;
use serde::{Deserialize, Serialize};

/// Where accepted messages go.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MailerMode {
    /// No transport: the delivery hook marks queued messages sent without
    /// delivering them.
    #[default]
    Mocked,
    /// Messages are stored `queued` and delivered by the SMTP transport.
    Smtp,
}

/// `@mailer/transport`.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct MailerConfig {
    pub mode: MailerMode,
    /// A message queued this long ago that the transport never attempted
    /// is marked failed: its `EmailQueued` was lost.
    pub stale_after_secs: u32,
    /// Seconds. The retention job deletes the body of a message created this
    /// long ago, unless it is still queued or held.
    pub delete_mail_older_than: u32,
}

impl Default for MailerConfig {
    fn default() -> Self {
        Self {
            mode: MailerMode::default(),
            stale_after_secs: 60 * 60,
            delete_mail_older_than: 30 * 24 * 60 * 60,
        }
    }
}

impl ModuleConfig for MailerConfig {
    const CONFIG_PREFIX: &'static str = "@mailer";
    const CONFIG_KEY: &'static str = "transport";
}
