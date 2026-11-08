use std::collections::HashMap;

use pipedash_plugin_api::config::{
    http_base_url,
    invalid_pipeline_id,
};
#[cfg(target_os = "ios")]
use pipedash_plugin_api::PluginError;
use pipedash_plugin_api::PluginResult;

pub(crate) fn get_base_url(config: &HashMap<String, String>) -> PluginResult<String> {
    let (url, _parsed) = http_base_url(
        config,
        "base_url",
        "Missing Woodpecker base URL",
        "Woodpecker",
    )?;
    #[cfg(target_os = "ios")]
    if _parsed.scheme() != "https" {
        return Err(PluginError::InvalidConfig(
            "Woodpecker on iOS requires an HTTPS server URL, such as https://ci.example.com. Enable HTTPS on your server before adding or importing this provider."
                .to_string(),
        ));
    }
    Ok(url.to_string())
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<i64> {
    id.parse().map_err(|_| invalid_pipeline_id(id))
}

pub(crate) fn pipeline_web_url(base_url: &str, repo_id: i64, number: i64) -> String {
    format!("{base_url}/repos/{repo_id}/pipeline/{number}")
}

#[cfg(test)]
mod tests;
