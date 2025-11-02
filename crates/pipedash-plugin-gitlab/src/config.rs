use std::collections::HashMap;

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<(i64, i64)> {
    let parts: Vec<&str> = id.split("__").collect();

    if parts.len() < 3 || parts[0] != "gitlab" {
        return Err(PluginError::InvalidConfig(format!(
            "Invalid pipeline ID format: {}",
            id
        )));
    }

    let provider_id = parts[1]
        .parse::<i64>()
        .map_err(|_| PluginError::InvalidConfig(format!("Invalid provider ID in: {}", id)))?;

    let project_id = parts[2]
        .parse::<i64>()
        .map_err(|_| PluginError::InvalidConfig(format!("Invalid project ID in: {}", id)))?;

    Ok((provider_id, project_id))
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

pub(crate) fn get_base_url(config: &HashMap<String, String>) -> String {
    config
        .get("base_url")
        .and_then(|url| {
            let trimmed = url.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.trim_end_matches('/').to_string())
            }
        })
        .unwrap_or_else(|| "https://gitlab.com".to_string())
}

pub(crate) fn build_api_url(base_url: &str) -> String {
    format!("{}/api/v4", base_url)
}
