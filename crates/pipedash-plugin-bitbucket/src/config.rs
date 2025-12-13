use std::collections::HashMap;

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

/// Format: bitbucket__{provider_id}__{workspace}__{repo_slug}
pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<(i64, String, String)> {
    let parts: Vec<&str> = id.split("__").collect();

    if parts.len() < 4 || parts[0] != "bitbucket" {
        return Err(PluginError::InvalidConfig(format!(
            "Invalid pipeline ID format: {}",
            id
        )));
    }

    let provider_id = parts[1]
        .parse::<i64>()
        .map_err(|_| PluginError::InvalidConfig(format!("Invalid provider ID in: {}", id)))?;

    let workspace = parts[2].to_string();
    let repo_slug = parts[3].to_string();

    Ok((provider_id, workspace, repo_slug))
}

pub(crate) fn parse_selected_items(config: &HashMap<String, String>) -> Option<Vec<String>> {
    config.get("selected_items").map(|items| {
        items
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    })
}

/// Only Bitbucket Cloud supported (Data Center has different API, no Pipelines)
pub(crate) fn get_api_url() -> String {
    "https://api.bitbucket.org/2.0".to_string()
}

/// API tokens replaced app passwords Sept 2025 (disabled June 2026)
pub(crate) fn get_auth(config: &HashMap<String, String>) -> PluginResult<(String, String)> {
    let email = config
        .get("email")
        .ok_or_else(|| PluginError::InvalidConfig("Missing Atlassian account email".to_string()))?
        .clone();

    let api_token = config
        .get("api_token")
        .ok_or_else(|| PluginError::InvalidConfig("Missing Bitbucket API token".to_string()))?
        .clone();

    Ok((email, api_token))
}
