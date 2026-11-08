use std::collections::HashMap;

use pipedash_plugin_api::config::{
    csv_list,
    selected_items,
    PIPELINE_ID_SEPARATOR,
};
use pipedash_plugin_api::kubernetes::KubeconfigSource;
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

pub(crate) fn expand_path(path: &str) -> String {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped).to_string_lossy().to_string();
        }
    }

    let expanded = shellexpand::env(path).unwrap_or(std::borrow::Cow::Borrowed(path));
    expanded.to_string()
}

pub(crate) fn get_default_kubeconfig_path() -> String {
    if let Ok(kubeconfig_env) = std::env::var("KUBECONFIG") {
        if !kubeconfig_env.trim().is_empty() {
            return kubeconfig_env;
        }
    }

    if let Some(home) = dirs::home_dir() {
        return home.join(".kube/config").to_string_lossy().to_string();
    }

    "~/.kube/config".to_string()
}

pub(crate) fn split_kubeconfig_paths(path: &str) -> Vec<String> {
    let separator = if cfg!(windows) { ';' } else { ':' };

    path.split(separator)
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .map(expand_path)
        .collect()
}

pub(crate) fn get_kubeconfig_path(config: &HashMap<String, String>) -> Option<String> {
    config
        .get("kubeconfig_path")
        .filter(|path| !path.trim().is_empty())
        .map(|path| expand_path(path))
}

pub(crate) const KUBECONFIG_CONTENT_KEY: &str = "token";

pub(crate) fn get_kubeconfig_source(
    config: &HashMap<String, String>,
) -> KubeconfigSource<Option<String>> {
    KubeconfigSource::from_config(config, KUBECONFIG_CONTENT_KEY, || {
        get_kubeconfig_path(config)
    })
}

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
