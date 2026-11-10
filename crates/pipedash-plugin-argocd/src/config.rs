use std::collections::HashMap;

use pipedash_plugin_api::config::{
    csv_list,
    required_base_url,
    value,
    PIPELINE_ID_SEPARATOR,
};
use pipedash_plugin_api::kubernetes::{
    self,
    KubeconfigSource,
};
use pipedash_plugin_api::utils::repository_name;
use pipedash_plugin_api::{
    AuthMode,
    PluginError,
    PluginResult,
};

pub(crate) const KUBECONFIG_PATH_KEY: &str = "kubeconfig";

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<(String, String)> {
    let (namespace, app_name) = id
        .split_once(PIPELINE_ID_SEPARATOR)
        .filter(|(_, app_name)| !app_name.contains(PIPELINE_ID_SEPARATOR))
        .ok_or_else(|| {
            PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: '{id}'. Expected format: '{{namespace}}__{{app_name}}'"
            ))
        })?;
    Ok((namespace.to_string(), app_name.to_string()))
}

pub(crate) fn get_server_url(config: &HashMap<String, String>) -> PluginResult<String> {
    required_base_url(config, "server_url", "Missing server_url in config").map(str::to_string)
}

pub(crate) fn parse_organizations_filter(config: &HashMap<String, String>) -> Option<Vec<String>> {
    csv_list(config, "organizations")
}

pub(crate) fn build_pipeline_id(namespace: &str, app_name: &str) -> String {
    format!("{namespace}__{app_name}")
}

pub(crate) fn uses_kubeconfig(config: &HashMap<String, String>) -> bool {
    AuthMode::from_config(config) == Some(AuthMode::Kubeconfig)
}

pub(crate) struct ClusterSettings {
    pub source: KubeconfigSource,
    pub context: Option<String>,
    pub namespaces: Vec<String>,
}

impl ClusterSettings {
    pub fn from_config(config: &HashMap<String, String>) -> PluginResult<Self> {
        let namespaces = csv_list(config, "namespaces").unwrap_or_default();
        if let Some(invalid) = namespaces
            .iter()
            .find(|namespace| !kubernetes::is_namespace(namespace))
        {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid namespace '{invalid}' in 'namespaces': expected comma-separated Kubernetes namespace names"
            )));
        }

        Ok(Self {
            source: KubeconfigSource::from_config(config, KUBECONFIG_PATH_KEY),
            context: value(config, "context").map(str::to_string),
            namespaces,
        })
    }
}

pub(crate) fn extract_git_org(repo_url: &str) -> String {
    let repo_name = repository_name(repo_url);
    repo_name.split('/').next().unwrap_or("unknown").to_string()
}

#[cfg(test)]
mod tests;
