use std::collections::HashMap;

use pipedash_plugin_api::config::{
    base_url,
    csv_list,
};
use pipedash_plugin_api::http::parse_http_url;
use pipedash_plugin_api::{
    custom_status_source,
    ComponentFilter,
    FeedFormat,
    PluginError,
    PluginResult,
    StatusSource,
};
use reqwest::Url;

const GITHUB_STATUS_COMPONENTS: [&str; 4] = [
    "br0l2tvcx85d",
    "brv1bkgrwx7q",
    "8l4ygp009s5s",
    "4230lsnqdsld",
];

pub(crate) const DYNAMIC_WORKFLOW_PREFIX: &str = "dynamic/";

pub(crate) fn get_repositories(config: &HashMap<String, String>) -> Vec<String> {
    csv_list(config, "selected_items")
        .or_else(|| csv_list(config, "repositories")) // Fallback for backward compatibility
        .unwrap_or_default()
}

pub(crate) fn parse_repo(repo: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = repo.split('/').collect();
    if parts.len() == 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

pub(crate) fn get_base_url(config: &HashMap<String, String>) -> String {
    base_url(config, "base_url")
        .unwrap_or("https://github.com")
        .to_string()
}

fn parse_base_url(base_url: &str) -> PluginResult<Url> {
    parse_http_url(base_url, "GitHub base")
}

fn is_github_com(host: &str) -> bool {
    matches!(host, "github.com" | "www.github.com" | "api.github.com")
}

pub(crate) fn source_host(config: &HashMap<String, String>) -> Option<String> {
    let url = parse_base_url(&get_base_url(config)).ok()?;
    let host = url.host_str()?;
    let host = if is_github_com(host) {
        "github.com"
    } else {
        host
    };
    Some(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

pub(crate) fn source_hosts(config: &HashMap<String, String>) -> Vec<String> {
    if config.is_empty() {
        return Vec::new();
    }
    source_host(config).into_iter().collect()
}

pub(crate) fn pipeline_id(owner: &str, repo: &str, workflow_id: u64) -> String {
    format!("{owner}__{repo}__{workflow_id}")
}

pub(crate) fn build_api_url(base_url: &str) -> PluginResult<String> {
    let url = parse_base_url(base_url)?;
    let base = base_url.trim_end_matches('/');

    Ok(match url.host_str().unwrap_or_default() {
        "github.com" | "www.github.com" => "https://api.github.com".to_string(),
        "api.github.com" => base.to_string(),
        host if host.ends_with(".ghe.com") => format!("https://api.{host}"),
        _ => format!("{base}/api/v3"),
    })
}

pub(crate) fn status_source(
    config: &HashMap<String, String>, base_url: &str,
) -> PluginResult<Option<StatusSource>> {
    if let Some(source) = custom_status_source(config)? {
        return Ok(Some(source));
    }

    if !is_github_com(parse_base_url(base_url)?.host_str().unwrap_or_default()) {
        return Ok(None);
    }

    let url = |raw: &str| {
        Url::parse(raw).map_err(|e| PluginError::Internal(format!("Invalid status URL: {e}")))
    };
    Ok(Some(StatusSource {
        name: "GitHub".to_string(),
        page_url: url("https://www.githubstatus.com/")?,
        feed_url: url("https://www.githubstatus.com/api/v2/summary.json")?,
        format: FeedFormat::Statuspage,
        components: ComponentFilter::Ids(
            GITHUB_STATUS_COMPONENTS
                .iter()
                .map(|id| id.to_string())
                .collect(),
        ),
    }))
}

#[cfg(test)]
mod tests;
