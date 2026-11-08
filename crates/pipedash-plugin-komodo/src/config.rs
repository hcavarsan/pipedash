use std::collections::HashMap;

use pipedash_plugin_api::config::required_base_url;
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

pub(crate) fn get_server_url(config: &HashMap<String, String>) -> PluginResult<String> {
    required_base_url(config, "server_url", "Missing server_url in config").map(str::to_string)
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<&str> {
    if id.is_empty() {
        return Err(PluginError::InvalidConfig(format!(
            "Invalid pipeline ID format: '{id}'. Expected format: '{{stack_id}}'"
        )));
    }
    Ok(id)
}

#[cfg(test)]
mod tests;
