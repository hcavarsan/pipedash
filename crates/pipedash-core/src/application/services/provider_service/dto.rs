use std::collections::HashMap;

use pipedash_plugin_api::{
    FeatureAvailability,
    PermissionStatus,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::domain::ProviderConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderView {
    pub id: i64,
    pub name: String,
    pub provider_type: String,
    #[serde(default)]
    pub config: HashMap<String, String>,
    pub refresh_interval: i64,
}

impl ProviderView {
    pub fn new(id: i64, config: ProviderConfig) -> Self {
        Self {
            id,
            name: config.name,
            provider_type: config.provider_type,
            config: config.config,
            refresh_interval: config.refresh_interval,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionCheckResult {
    pub permission_status: Option<PermissionStatus>,
    pub features: Vec<FeatureAvailability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultPreferences {
    #[serde(rename = "columnOrder")]
    pub column_order: Vec<String>,
    #[serde(rename = "columnVisibility")]
    pub column_visibility: HashMap<String, bool>,
}
