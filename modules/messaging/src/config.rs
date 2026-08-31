//! Typed configuration for the `messaging` module.

use base::config::ConfigJson;
use serde::{Deserialize, Serialize};

/// SMTP transport settings. `Debug` redacts credentials.
#[derive(Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub sender: String,
    pub starttls: bool,
}

impl core::fmt::Debug for SmtpConfig {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SmtpConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("username", &"***")
            .field("password", &"***")
            .field("sender", &self.sender)
            .field("starttls", &self.starttls)
            .finish()
    }
}

impl Default for SmtpConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 587,
            username: String::new(),
            password: String::new(),
            sender: "noreply@example.com".to_string(),
            starttls: true,
        }
    }
}

/// Site branding used in emails.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteConfig {
    pub site_name: String,
    pub frontend_domain: String,
    pub logo_url: String,
}

impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            site_name: "Airi".to_string(),
            frontend_domain: "http://localhost:3000".to_string(),
            logo_url: String::new(),
        }
    }
}

/// Messaging configuration: SMTP + site branding.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessagingConfig {
    pub smtp: SmtpConfig,
    pub site: SiteConfig,
}

impl ConfigJson for MessagingConfig {
    const KEY: &'static str = "messaging";
}
