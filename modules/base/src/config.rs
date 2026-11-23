//! Module configuration.
//!
//! Put the strongly typed configuration for this module here. The convention in
//! this stack is to store configuration as JSON in the database (one row per
//! key in a shared application-config table) and cache it in Redis so services
//! can load it cheaply and read-only at runtime. The management CLI seeds the
//! defaults; a refresh step copies the database value into the Redis cache.
//!
//! Define a `serde`-(de)serializable struct that implements `Default` and bind
//! it to a stable config key:

use serde::{Deserialize, Serialize};
use crate::ModuleConfig;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RegisterMode {
    PublicAvailable,
    #[default]
    OnlyInvited,
    Close,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteMetadataConfig {
    pub site_name: String,
    pub site_description: String,
    pub site_register_mode: RegisterMode,
}

impl Default for SiteMetadataConfig {
    fn default() -> Self {
        Self {
            site_name: "Airi".to_owned(),
            site_description:
                "Open-source IdP for clubs and small business. This is the default description. Change the description to start yours."
                    .to_owned(),
            site_register_mode: RegisterMode::default(),
        }
    }
}

impl ModuleConfig for SiteMetadataConfig {
    const CONFIG_PREFIX: &'static str = "@base";
    const CONFIG_KEY: &'static str = "site_metadata";
}
