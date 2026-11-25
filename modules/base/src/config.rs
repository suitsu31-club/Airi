//! Module configuration.
//!
//! Put the strongly typed configuration for this module here. Configuration
//! is stored as JSON in `base.module_config` (one row per key) and cached in
//! Redis. Services read it with
//! [`ConfigLoader`](crate::services::config::ConfigLoader) and change it with
//! [`ConfigUpdater`](crate::services::config::ConfigUpdater), which drops the
//! cached copy; the management CLI seeds the defaults.
//!
//! Define a `serde`-(de)serializable struct that implements `Default` and bind
//! it to a stable config key with [`ModuleConfig`]:

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
