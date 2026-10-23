use std::{
    collections::{
        BTreeSet,
        HashMap,
    },
    fmt,
};

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

use crate::types::Kind;

pub(crate) const PROVIDER_TYPE: &str = "fluxcd";
pub(crate) const DEFAULT_FLUX_NAMESPACE: &str = "flux-system";
pub(crate) const CONTROLLERS: [&str; 3] = [
    "source-controller",
    "kustomize-controller",
    "helm-controller",
];

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Target {
    pub kind: Kind,
    pub namespace: String,
    pub name: String,
}

impl Target {
    pub fn new(kind: Kind, namespace: &str, name: &str) -> PluginResult<Self> {
        if !is_namespace(namespace) {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid namespace '{namespace}' for {kind}: expected a Kubernetes namespace name (lowercase alphanumerics and '-', max 63 characters)"
            )));
        }
        if !is_resource_name(name) {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid {kind} name '{name}' in namespace '{namespace}': expected a Kubernetes resource name (lowercase alphanumerics, '-' and '.', max 253 characters)"
            )));
        }

        Ok(Self {
            kind,
            namespace: namespace.to_string(),
            name: name.to_string(),
        })
    }

    pub fn selection_id(&self) -> String {
        format!("{}__{}__{}", self.kind, self.namespace, self.name)
    }

    pub fn pipeline_id(&self, provider_id: i64) -> String {
        format!("{PROVIDER_TYPE}__{provider_id}__{}", self.selection_id())
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}/{}", self.kind, self.namespace, self.name)
    }
}

fn is_dns_label(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub(crate) fn is_namespace(value: &str) -> bool {
    value.len() <= 63 && is_dns_label(value)
}

fn is_resource_name(value: &str) -> bool {
    value.len() <= 253 && value.split('.').all(is_dns_label)
}

pub(crate) fn parse_selection(item: &str) -> PluginResult<Target> {
    let parts: Vec<&str> = item.split("__").collect();
    let [kind, namespace, name] = parts.as_slice() else {
        return Err(PluginError::InvalidConfig(format!(
            "Invalid FluxCD selection '{item}'. Expected: Kind__namespace__name"
        )));
    };
    let kind = Kind::from_name(kind).ok_or_else(|| {
        PluginError::InvalidConfig(format!(
            "Invalid FluxCD selection '{item}': kind must be Kustomization or HelmRelease"
        ))
    })?;

    Target::new(kind, namespace, name)
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<(i64, Target)> {
    let parts: Vec<&str> = id.split("__").collect();
    let [prefix, provider_id, kind, namespace, name] = parts.as_slice() else {
        return Err(PluginError::InvalidConfig(format!(
            "Invalid FluxCD pipeline ID '{id}'. Expected: fluxcd__{{provider_id}}__{{Kind}}__{{namespace}}__{{name}}"
        )));
    };
    if *prefix != PROVIDER_TYPE {
        return Err(PluginError::InvalidConfig(format!(
            "Pipeline ID '{id}' does not belong to the FluxCD provider"
        )));
    }
    let provider_id = provider_id.parse::<i64>().map_err(|_| {
        PluginError::InvalidConfig(format!("Invalid provider ID in pipeline ID '{id}'"))
    })?;
    let kind = Kind::from_name(kind).ok_or_else(|| {
        PluginError::InvalidConfig(format!(
            "Invalid kind in pipeline ID '{id}': expected Kustomization or HelmRelease"
        ))
    })?;

    Ok((provider_id, Target::new(kind, namespace, name)?))
}

fn expand_path(path: &str) -> String {
    shellexpand::full(path)
        .map(|expanded| expanded.into_owned())
        .unwrap_or_else(|_| path.to_string())
}

pub(crate) fn default_kubeconfig() -> String {
    if let Ok(kubeconfig) = std::env::var("KUBECONFIG") {
        if !kubeconfig.trim().is_empty() {
            return kubeconfig;
        }
    }

    dirs::home_dir()
        .map(|home| home.join(".kube/config").to_string_lossy().to_string())
        .unwrap_or_else(|| "~/.kube/config".to_string())
}

fn split_kubeconfig_paths(paths: &str) -> Vec<String> {
    let separator = if cfg!(windows) { ';' } else { ':' };

    paths
        .split(separator)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(expand_path)
        .collect()
}

pub(crate) fn kubeconfig_paths(config: &HashMap<String, String>) -> Vec<String> {
    let configured = config
        .get("kubeconfig")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());

    match configured {
        Some(paths) => split_kubeconfig_paths(paths),
        None => split_kubeconfig_paths(&default_kubeconfig()),
    }
}

pub(crate) fn context(config: &HashMap<String, String>) -> Option<String> {
    config
        .get("context")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn comma_separated(config: &HashMap<String, String>, key: &str) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    for item in config
        .get(key)
        .map(String::as_str)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        if !items.iter().any(|existing| existing == item) {
            items.push(item.to_string());
        }
    }

    items
}

#[derive(Debug, Clone)]
pub(crate) struct Settings {
    pub kubeconfig_paths: Vec<String>,
    pub context: Option<String>,
    pub namespaces: Vec<String>,
    pub selection: Vec<Target>,
    pub flux_namespace: String,
}

impl Settings {
    pub fn from_config(config: &HashMap<String, String>) -> PluginResult<Self> {
        let namespaces = comma_separated(config, "namespaces");
        if let Some(invalid) = namespaces.iter().find(|namespace| !is_namespace(namespace)) {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid namespace '{invalid}' in 'namespaces': expected comma-separated Kubernetes namespace names"
            )));
        }

        let selection = comma_separated(config, "selected_items")
            .iter()
            .map(|item| parse_selection(item))
            .collect::<PluginResult<Vec<_>>>()?;

        let flux_namespace = config
            .get("flux_namespace")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_FLUX_NAMESPACE);
        if !is_namespace(flux_namespace) {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid namespace '{flux_namespace}' in 'flux_namespace': expected a Kubernetes namespace name"
            )));
        }

        let settings = Self {
            kubeconfig_paths: kubeconfig_paths(config),
            context: context(config),
            namespaces,
            selection,
            flux_namespace: flux_namespace.to_string(),
        };
        if let Some(outside) = settings
            .selection
            .iter()
            .find(|target| !settings.in_namespaces(target))
        {
            return Err(PluginError::InvalidConfig(format!(
                "Selected {outside} is outside the configured namespaces ({})",
                settings.namespaces.join(", ")
            )));
        }

        Ok(settings)
    }

    fn in_namespaces(&self, target: &Target) -> bool {
        self.namespaces.is_empty() || self.namespaces.contains(&target.namespace)
    }

    pub fn ensure_allowed(&self, target: &Target) -> PluginResult<()> {
        if !self.in_namespaces(target) {
            return Err(PluginError::InvalidConfig(format!(
                "{target} is outside the configured namespaces ({})",
                self.namespaces.join(", ")
            )));
        }
        if !self.selection.is_empty() && !self.selection.contains(target) {
            return Err(PluginError::InvalidConfig(format!(
                "{target} is not part of this provider's selected items"
            )));
        }

        Ok(())
    }

    pub fn monitored_namespaces(&self) -> Vec<String> {
        if self.selection.is_empty() {
            return self.namespaces.clone();
        }

        self.selection
            .iter()
            .map(|target| target.namespace.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(str::to_string)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    #[test]
    fn pipeline_id_round_trips_kind_namespace_and_name() {
        let target = Target::new(Kind::HelmRelease, "apps", "web.v2").unwrap();

        let (provider_id, parsed) = parse_pipeline_id(&target.pipeline_id(7)).unwrap();

        assert_eq!((provider_id, parsed), (7, target));
    }

    #[test]
    fn pipeline_id_rejects_other_providers_and_malformed_identities() {
        for id in [
            "tekton__1__Kustomization__ci__app",
            "fluxcd__1__Kustomization__ci",
            "fluxcd__1__Kustomization__ci__app__extra",
            "fluxcd__x__Kustomization__ci__app",
            "fluxcd__1__kustomization__ci__app",
            "fluxcd__1__Kustomization__CI__app",
            "fluxcd__1__Kustomization__ci__app_name",
            "fluxcd__1__Kustomization__ci__..",
            "fluxcd__1__Kustomization____app",
        ] {
            assert!(parse_pipeline_id(id).is_err(), "{id} must be rejected");
        }
    }

    #[test]
    fn selection_requires_exactly_kind_namespace_and_name() {
        assert!(parse_selection("Kustomization__ci__app").is_ok());
        assert!(parse_selection("ci__app").is_err());
        assert!(parse_selection("Deployment__ci__app").is_err());
        assert!(parse_selection("HelmRelease__ci/other__app").is_err());
    }

    #[test]
    fn settings_split_comma_separated_namespaces_and_selection_without_duplicates() {
        let settings = Settings::from_config(&config(&[
            ("namespaces", "ci, apps,ci"),
            (
                "selected_items",
                "Kustomization__ci__app, HelmRelease__apps__web,Kustomization__ci__app",
            ),
        ]))
        .unwrap();

        assert_eq!(settings.namespaces, vec!["ci", "apps"]);
        assert_eq!(settings.selection.len(), 2);
        assert_eq!(settings.monitored_namespaces(), vec!["apps", "ci"]);
    }

    #[test]
    fn settings_reject_invalid_namespaces_and_selection_outside_them() {
        assert!(Settings::from_config(&config(&[("namespaces", "ci,Bad_NS")])).is_err());
        assert!(Settings::from_config(&config(&[
            ("namespaces", "ci"),
            ("selected_items", "Kustomization__other__app"),
        ]))
        .is_err());
    }

    #[test]
    fn empty_namespaces_monitor_all_and_allow_any_valid_target() {
        let settings = Settings::from_config(&config(&[])).unwrap();
        let target = Target::new(Kind::Kustomization, "anywhere", "app").unwrap();

        assert!(settings.monitored_namespaces().is_empty());
        assert!(settings.ensure_allowed(&target).is_ok());
    }

    #[test]
    fn selection_limits_which_targets_are_allowed() {
        let settings =
            Settings::from_config(&config(&[("selected_items", "Kustomization__ci__app")]))
                .unwrap();
        let selected = Target::new(Kind::Kustomization, "ci", "app").unwrap();
        let sibling = Target::new(Kind::Kustomization, "ci", "other").unwrap();
        let same_name_other_kind = Target::new(Kind::HelmRelease, "ci", "app").unwrap();

        assert!(settings.ensure_allowed(&selected).is_ok());
        assert!(settings.ensure_allowed(&sibling).is_err());
        assert!(settings.ensure_allowed(&same_name_other_kind).is_err());
    }
}
