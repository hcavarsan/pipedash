use std::collections::HashMap;

use pipedash_plugin_api::config::{
    csv_list,
    required_base_url,
    PIPELINE_ID_SEPARATOR,
};
use pipedash_plugin_api::utils::repository_name;
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

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

pub(crate) fn extract_git_org(repo_url: &str) -> String {
    let repo_name = repository_name(repo_url);
    repo_name.split('/').next().unwrap_or("unknown").to_string()
}

#[cfg(test)]
mod tests;
