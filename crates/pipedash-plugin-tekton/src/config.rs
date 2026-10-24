use std::{
    collections::HashMap,
    fmt,
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

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum KubeconfigSource {
    Content(String),
    Path(Option<String>),
}

impl fmt::Debug for KubeconfigSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Content(_) => f.write_str("Content(<redacted>)"),
            Self::Path(path) => f.debug_tuple("Path").field(path).finish(),
        }
    }
}

pub(crate) fn get_kubeconfig_source(config: &HashMap<String, String>) -> KubeconfigSource {
    match config
        .get(KUBECONFIG_CONTENT_KEY)
        .filter(|content| !content.trim().is_empty())
    {
        Some(content) => KubeconfigSource::Content(content.clone()),
        None => KubeconfigSource::Path(get_kubeconfig_path(config)),
    }
}

pub(crate) fn get_context(config: &HashMap<String, String>) -> Option<String> {
    config
        .get("context")
        .filter(|ctx| !ctx.trim().is_empty())
        .cloned()
}

pub(crate) fn get_selected_pipelines(config: &HashMap<String, String>) -> Vec<String> {
    config
        .get("selected_items")
        .map(|items| {
            items
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn get_namespace_mode(config: &HashMap<String, String>) -> NamespaceMode {
    config
        .get("namespace_mode")
        .filter(|mode| !mode.trim().is_empty())
        .map(|mode| NamespaceMode::from_config_value(mode))
        .unwrap_or(NamespaceMode::All)
}

pub(crate) fn get_namespaces(config: &HashMap<String, String>) -> Vec<String> {
    config
        .get("namespaces")
        .map(|items| {
            items
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
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

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<(i64, String, String)> {
    let parts: Vec<&str> = id.split("__").collect();

    if parts.len() < 4 || parts[0] != "tekton" {
        return Err(PluginError::InvalidConfig(format!(
            "Invalid Tekton pipeline ID format: {}. Expected: tekton__{{provider_id}}__{{namespace}}__{{pipeline_name}}",
            id
        )));
    }

    let provider_id = parts[1].parse::<i64>().map_err(|_| {
        PluginError::InvalidConfig(format!("Invalid provider ID in pipeline ID: {}", id))
    })?;

    let namespace = parts[2].to_string();
    let pipeline_name = parts[3..].join("__");

    Ok((provider_id, namespace, pipeline_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pipeline_id() {
        let id = "tekton__1__default__my-pipeline";
        let (provider_id, namespace, pipeline_name) = parse_pipeline_id(id).unwrap();
        assert_eq!(provider_id, 1);
        assert_eq!(namespace, "default");
        assert_eq!(pipeline_name, "my-pipeline");
    }

    #[test]
    fn test_parse_pipeline_id_with_dashes() {
        let id = "tekton__2__tekton-pipelines__complex-pipeline-name";
        let (provider_id, namespace, pipeline_name) = parse_pipeline_id(id).unwrap();
        assert_eq!(provider_id, 2);
        assert_eq!(namespace, "tekton-pipelines");
        assert_eq!(pipeline_name, "complex-pipeline-name");
    }

    #[test]
    fn test_parse_invalid_pipeline_id() {
        let id = "invalid__1__namespace__pipeline";
        assert!(parse_pipeline_id(id).is_err());
    }

    fn config(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn kubeconfig_content_is_parsed_from_config() {
        let source = get_kubeconfig_source(&config(&[("token", "apiVersion: v1\nkind: Config\n")]));

        assert_eq!(
            source,
            KubeconfigSource::Content("apiVersion: v1\nkind: Config\n".to_string())
        );
    }

    #[test]
    fn kubeconfig_content_wins_over_the_path() {
        let source = get_kubeconfig_source(&config(&[
            ("kubeconfig_path", "/home/me/.kube/config"),
            ("token", "apiVersion: v1"),
        ]));

        assert_eq!(
            source,
            KubeconfigSource::Content("apiVersion: v1".to_string())
        );
    }

    #[test]
    fn blank_content_falls_back_to_the_path() {
        let source = get_kubeconfig_source(&config(&[
            ("kubeconfig_path", "/home/me/.kube/config"),
            ("token", "  \n "),
        ]));

        assert_eq!(
            source,
            KubeconfigSource::Path(Some("/home/me/.kube/config".to_string()))
        );
    }

    #[test]
    fn kubeconfig_content_is_never_printed() {
        let source = get_kubeconfig_source(&config(&[("token", "token: s3cr3t-value")]));

        assert!(!format!("{source:?}").contains("s3cr3t-value"));
    }
}
