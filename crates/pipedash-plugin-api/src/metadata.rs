use serde::{
    Deserialize,
    Serialize,
};

use crate::history::RunHistoryPolicy;
use crate::presentation::PluginPresentation;
use crate::schema::{
    ConfigSchema,
    TableSchema,
};
use crate::types::{
    Feature,
    Permission,
};
use crate::utils::{
    provider_type,
    svg_data_url,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub name: String,
    pub provider_type: String,
    pub version: String,
    pub description: String,
    pub author: Option<String>,
    pub icon: Option<String>,
    pub config_schema: ConfigSchema,
    pub table_schema: TableSchema,
    pub capabilities: PluginCapabilities,
    #[serde(default)]
    pub required_permissions: Vec<Permission>,
    #[serde(default)]
    pub features: Vec<Feature>,
    #[serde(default)]
    pub run_history: RunHistoryPolicy,
    #[serde(default)]
    pub token_prefixes: Vec<String>,
    pub presentation: PluginPresentation,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginCapabilities {
    pub trigger: bool,
    pub cancel: bool,
    #[serde(default)]
    pub force_cancel: bool,
    #[serde(default)]
    pub retains_run_history: bool,
    #[serde(default)]
    pub logs: bool,
    #[serde(default)]
    pub sign_in: bool,
    #[serde(default)]
    pub uses_http_proxy: bool,
    #[serde(default)]
    pub async_trigger: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct PluginManifest {
    pub package_name: &'static str,
    pub version: &'static str,
    pub description: &'static str,
    pub authors: &'static str,
    pub icon_svg: &'static str,
}

impl PluginManifest {
    pub fn provider_type(&self) -> &'static str {
        provider_type(self.package_name)
    }

    pub(crate) fn apply(&self, metadata: &mut PluginMetadata) {
        metadata.provider_type = self.provider_type().to_string();
        metadata.version = self.version.to_string();
        metadata.description = self.description.to_string();
        metadata.author = Some(self.authors.to_string());
        metadata.icon = Some(svg_data_url(self.icon_svg));
    }
}
