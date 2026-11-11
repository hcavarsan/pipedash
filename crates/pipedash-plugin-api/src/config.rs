use std::collections::HashMap;

use reqwest::Url;

use crate::error::{
    PluginError,
    PluginResult,
};
use crate::http::parse_http_url;

pub const PIPELINE_ID_SEPARATOR: &str = "__";

pub fn value<'a>(config: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    config.get(key).map(|v| v.trim()).filter(|v| !v.is_empty())
}

pub fn required_value<'a>(
    config: &'a HashMap<String, String>, key: &str, missing_message: &str,
) -> PluginResult<&'a str> {
    value(config, key).ok_or_else(|| PluginError::InvalidConfig(missing_message.to_string()))
}

pub fn credential(config: &HashMap<String, String>, key: &str) -> PluginResult<String> {
    required_value(config, key, &format!("Missing {key} in config")).map(str::to_string)
}

pub fn comma_separated(value: &str) -> impl Iterator<Item = &str> + '_ {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
}

pub fn csv_list(config: &HashMap<String, String>, key: &str) -> Option<Vec<String>> {
    config
        .get(key)
        .map(|items| comma_separated(items).map(str::to_string).collect())
}

pub fn selected_items(config: &HashMap<String, String>) -> Option<Vec<String>> {
    csv_list(config, "selected_items")
}

pub fn bool_value(config: &HashMap<String, String>, key: &str) -> bool {
    config
        .get(key)
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false)
}

pub fn insecure(config: &HashMap<String, String>) -> bool {
    bool_value(config, "insecure")
}

pub fn base_url<'a>(config: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    value(config, key)
        .map(|url| url.trim_end_matches('/'))
        .filter(|url| !url.is_empty())
}

pub fn required_base_url<'a>(
    config: &'a HashMap<String, String>, key: &str, missing_message: &str,
) -> PluginResult<&'a str> {
    base_url(config, key).ok_or_else(|| PluginError::InvalidConfig(missing_message.to_string()))
}

pub fn http_base_url<'a>(
    config: &'a HashMap<String, String>, key: &str, missing_message: &str, label: &str,
) -> PluginResult<(&'a str, Url)> {
    let raw = required_base_url(config, key, missing_message)?;
    let parsed = parse_http_url(raw, label)?;
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(PluginError::InvalidConfig(format!(
            "Invalid {label} URL: remove the query or fragment"
        )));
    }
    Ok((raw, parsed))
}

pub fn invalid_pipeline_id(id: &str) -> PluginError {
    PluginError::InvalidConfig(format!("Invalid pipeline ID format: {id}"))
}

#[cfg(test)]
mod tests;
