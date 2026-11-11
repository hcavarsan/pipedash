use std::collections::HashMap;

use pipedash_plugin_api::config::{
    base_url,
    PIPELINE_ID_SEPARATOR,
};
use pipedash_plugin_api::{
    custom_status_source,
    ComponentFilter,
    FeedFormat,
    PluginError,
    PluginResult,
    RepoId,
    StatusSource,
};

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<i64> {
    id.split(PIPELINE_ID_SEPARATOR)
        .next()
        .and_then(|project_id| project_id.parse().ok())
        .ok_or_else(|| PluginError::InvalidConfig(format!("Invalid project ID in: {id}")))
}

pub(crate) fn get_base_url(config: &HashMap<String, String>) -> String {
    base_url(config, "base_url")
        .unwrap_or("https://gitlab.com")
        .to_string()
}

pub(crate) fn source_host(config: &HashMap<String, String>) -> Option<String> {
    RepoId::parse(&format!("{}/_", get_base_url(config))).map(|repo| repo.host().to_string())
}

pub(crate) fn source_hosts(config: &HashMap<String, String>) -> Vec<String> {
    if config.is_empty() {
        return Vec::new();
    }
    source_host(config).into_iter().collect()
}

pub(crate) fn registry_host(config: &HashMap<String, String>) -> Option<String> {
    source_host(config).map(|host| format!("registry.{host}"))
}

pub(crate) fn build_api_url(base_url: &str) -> String {
    format!("{}/api/v4", base_url)
}

pub(crate) fn status_source(
    config: &HashMap<String, String>,
) -> PluginResult<Option<StatusSource>> {
    if let Some(custom) = custom_status_source(config)? {
        return Ok(Some(custom));
    }
    let is_saas = reqwest::Url::parse(&get_base_url(config))
        .is_ok_and(|url| url.host_str() == Some("gitlab.com"));
    if !is_saas {
        return Ok(None);
    }
    let url = |value: &str| {
        reqwest::Url::parse(value)
            .map_err(|e| PluginError::Internal(format!("Invalid GitLab status URL: {e}")))
    };
    Ok(Some(StatusSource {
        name: "GitLab".to_string(),
        page_url: url("https://status.gitlab.com/")?,
        feed_url: url("https://api.status.io/1.0/status/5b36dc6502d06804c08349f7")?,
        format: FeedFormat::StatusIo,
        components: ComponentFilter::Ids(
            [
                "5b36e05f02d06804c0834a09",
                "5d2f74932676bc45e4927ead",
                "67ea346990c050053ab18e89",
                "5b36e06c633e9004b3d624ad",
                "5e29c1403f4deb04c0d7f035",
                "650c87fbadb04b052efa2bc9",
            ]
            .map(str::to_string)
            .to_vec(),
        ),
    }))
}

#[cfg(test)]
mod tests;
