use std::collections::HashMap;

use pipedash_plugin_api::config::comma_separated;
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

pub(crate) fn parse_selected_items(
    config: &HashMap<String, String>,
) -> PluginResult<Vec<(String, String)>> {
    let selected_items = config
        .get("selected_items")
        .or_else(|| config.get("pipelines")) // Fallback for backward compatibility
        .ok_or_else(|| PluginError::InvalidConfig("No pipelines selected".to_string()))?;

    let mut pipelines = Vec::new();

    for trimmed in comma_separated(selected_items) {
        let parts: Vec<&str> = trimmed.split('/').collect();
        match parts.as_slice() {
            [org, slug] => pipelines.push((org.to_string(), slug.to_string())),
            [slug] => {
                let org = config.get("organization").cloned().ok_or_else(|| {
                    PluginError::InvalidConfig(format!(
                        "Could not determine organization of pipeline {slug}"
                    ))
                })?;
                pipelines.push((org, slug.to_string()));
            }
            _ => {}
        }
    }

    Ok(pipelines)
}

#[cfg(test)]
mod tests;
