use std::collections::HashMap;

use pipedash_plugin_api::config::base_url;
use pipedash_plugin_api::PluginResult;

use crate::client::validate_project_slug;

pub(crate) const DEFAULT_BASE_URL: &str = "https://circleci.com";

pub(crate) fn get_base_url(config: &HashMap<String, String>) -> String {
    base_url(config, "base_url")
        .unwrap_or(DEFAULT_BASE_URL)
        .to_string()
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<&str> {
    validate_project_slug(id)?;
    Ok(id)
}

#[cfg(test)]
mod tests;
