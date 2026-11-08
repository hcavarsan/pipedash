use std::collections::HashMap;

use pipedash_plugin_api::config::{
    csv_list,
    selected_items,
    PIPELINE_ID_SEPARATOR,
};
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NamespaceMode {
    All,

    Custom,
}

impl NamespaceMode {
    pub fn from_config_value(value: &str) -> Self {
        match value.trim().to_lowercase().as_str() {
            "custom" => NamespaceMode::Custom,
            _ => NamespaceMode::All,
        }
    }
}

pub(crate) const KUBECONFIG_PATH_KEY: &str = "kubeconfig_path";

pub(crate) fn get_context(config: &HashMap<String, String>) -> Option<String> {
    config
        .get("context")
        .filter(|ctx| !ctx.trim().is_empty())
        .cloned()
}

pub(crate) fn get_selected_pipelines(config: &HashMap<String, String>) -> Vec<String> {
    selected_items(config).unwrap_or_default()
}

pub(crate) fn get_namespace_mode(config: &HashMap<String, String>) -> NamespaceMode {
    config
        .get("namespace_mode")
        .filter(|mode| !mode.trim().is_empty())
        .map(|mode| NamespaceMode::from_config_value(mode))
        .unwrap_or(NamespaceMode::All)
}

pub(crate) fn get_namespaces(config: &HashMap<String, String>) -> Vec<String> {
    csv_list(config, "namespaces").unwrap_or_default()
}

pub(crate) const DEFAULT_CONTROLLER_NAMESPACE: &str = "tekton-pipelines";

pub(crate) fn get_controller_namespace(config: &HashMap<String, String>) -> String {
    config
        .get("tekton_namespace")
        .map(|namespace| namespace.trim())
        .filter(|namespace| !namespace.is_empty())
        .unwrap_or(DEFAULT_CONTROLLER_NAMESPACE)
        .to_string()
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<(String, String)> {
    id.split_once(PIPELINE_ID_SEPARATOR)
        .map(|(namespace, pipeline_name)| (namespace.to_string(), pipeline_name.to_string()))
        .ok_or_else(|| {
            PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: '{id}'. Expected format: '{{namespace}}__{{pipeline_name}}'"
            ))
        })
}

const MAX_KUBE_NAME_BYTES: usize = 253;

pub(crate) fn is_kube_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    let alphanumeric = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    bytes.len() <= MAX_KUBE_NAME_BYTES
        && bytes.first().is_some_and(alphanumeric)
        && bytes.last().is_some_and(alphanumeric)
        && bytes
            .iter()
            .all(|b| alphanumeric(b) || *b == b'-' || *b == b'.')
}

#[cfg(test)]
mod tests;
