use std::{
    collections::{
        BTreeSet,
        HashMap,
    },
    fmt,
};

use pipedash_plugin_api::config::{
    comma_separated,
    value,
};
use pipedash_plugin_api::kubernetes::KubeconfigSource;
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};

use crate::types::Kind;

pub(crate) const DEFAULT_FLUX_NAMESPACE: &str = "flux-system";
pub(crate) const KUBECONFIG_PATH_KEY: &str = "kubeconfig";
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
        format!(
            "{kind}__{namespace}__{name}",
            kind = self.kind,
            namespace = self.namespace,
            name = self.name
        )
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

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<Target> {
    let invalid = || {
        PluginError::InvalidConfig(format!(
            "Invalid pipeline ID format: '{id}'. Expected format: '{{Kind}}__{{namespace}}__{{name}}'"
        ))
    };
    let parts: Vec<&str> = id.split("__").collect();
    let [kind, namespace, name] = parts.as_slice() else {
        return Err(invalid());
    };
    let kind = Kind::from_name(kind).ok_or_else(|| {
        PluginError::InvalidConfig(format!(
            "Invalid kind in pipeline ID '{id}': expected Kustomization or HelmRelease"
        ))
    })?;

    Target::new(kind, namespace, name)
}

pub(crate) fn context(config: &HashMap<String, String>) -> Option<String> {
    value(config, "context").map(str::to_string)
}

fn unique_csv(config: &HashMap<String, String>, key: &str) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    for item in comma_separated(config.get(key).map_or("", String::as_str)) {
        if !items.iter().any(|existing| existing == item) {
            items.push(item.to_string());
        }
    }

    items
}

#[derive(Debug, Clone)]
pub(crate) struct Settings {
    pub kubeconfig: KubeconfigSource,
    pub context: Option<String>,
    pub namespaces: Vec<String>,
    pub selection: Vec<Target>,
    pub flux_namespace: String,
}

impl Settings {
    pub fn from_config(config: &HashMap<String, String>) -> PluginResult<Self> {
        let namespaces = unique_csv(config, "namespaces");
        if let Some(invalid) = namespaces.iter().find(|namespace| !is_namespace(namespace)) {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid namespace '{invalid}' in 'namespaces': expected comma-separated Kubernetes namespace names"
            )));
        }

        let selection = unique_csv(config, "selected_items")
            .iter()
            .map(|item| parse_selection(item))
            .collect::<PluginResult<Vec<_>>>()?;

        let flux_namespace = value(config, "flux_namespace").unwrap_or(DEFAULT_FLUX_NAMESPACE);
        if !is_namespace(flux_namespace) {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid namespace '{flux_namespace}' in 'flux_namespace': expected a Kubernetes namespace name"
            )));
        }

        let settings = Self {
            kubeconfig: KubeconfigSource::from_config(config, KUBECONFIG_PATH_KEY),
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
mod tests;
