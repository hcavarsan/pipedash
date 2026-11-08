use std::collections::HashMap;

use pipedash_plugin_api::config::{
    http_base_url,
    invalid_pipeline_id,
};
use pipedash_plugin_api::PluginResult;

pub(crate) fn get_server_url(config: &HashMap<String, String>) -> PluginResult<String> {
    http_base_url(
        config,
        "server_url",
        "Missing server_url in config",
        "Coolify",
    )
    .map(|(url, _)| url.to_string())
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<&str> {
    if id.is_empty() {
        return Err(invalid_pipeline_id(id));
    }
    Ok(id)
}

#[cfg(test)]
mod tests;
