use std::collections::{
    HashMap,
    HashSet,
};

use pipedash_plugin_api::config::{
    csv_list,
    invalid_pipeline_id,
    selected_items,
    value,
    PIPELINE_ID_SEPARATOR,
};
use pipedash_plugin_api::PluginResult;

pub(crate) const KUBECONFIG_PATH_KEY: &str = "kubeconfig_path";
pub(crate) const CONTEXT_KEY: &str = "context";
pub(crate) const NAMESPACES_KEY: &str = "namespaces";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    Template,
    Cron,
}

impl Kind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Template => "template",
            Self::Cron => "cron",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Template => "Workflow template",
            Self::Cron => "Cron workflow",
        }
    }

    pub fn resource(self) -> &'static str {
        match self {
            Self::Template => "workflowtemplates",
            Self::Cron => "cronworkflows",
        }
    }

    pub fn run_label(self) -> &'static str {
        match self {
            Self::Template => "workflows.argoproj.io/workflow-template",
            Self::Cron => "workflows.argoproj.io/cron-workflow",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        [Self::Template, Self::Cron]
            .into_iter()
            .find(|kind| kind.id() == value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PipelineRef {
    pub namespace: String,
    pub kind: Kind,
    pub name: String,
}

impl PipelineRef {
    pub fn new(namespace: &str, kind: Kind, name: &str) -> Self {
        Self {
            namespace: namespace.to_string(),
            kind,
            name: name.to_string(),
        }
    }

    pub fn id(&self) -> String {
        [self.namespace.as_str(), self.kind.id(), self.name.as_str()].join(PIPELINE_ID_SEPARATOR)
    }
}

pub(crate) fn context(config: &HashMap<String, String>) -> Option<String> {
    value(config, CONTEXT_KEY).map(str::to_string)
}

pub(crate) fn namespaces(config: &HashMap<String, String>) -> Vec<String> {
    csv_list(config, NAMESPACES_KEY).unwrap_or_default()
}

pub(crate) fn selected(config: &HashMap<String, String>) -> HashSet<PipelineRef> {
    selected_items(config)
        .unwrap_or_default()
        .iter()
        .filter_map(|id| parse_pipeline_id(id).ok())
        .collect()
}

pub(crate) fn parse_pipeline_id(id: &str) -> PluginResult<PipelineRef> {
    let mut parts = id.splitn(3, PIPELINE_ID_SEPARATOR);
    match (
        parts.next(),
        parts.next().and_then(Kind::parse),
        parts.next(),
    ) {
        (Some(namespace), Some(kind), Some(name)) if !namespace.is_empty() && !name.is_empty() => {
            Ok(PipelineRef::new(namespace, kind, name))
        }
        _ => Err(invalid_pipeline_id(id)),
    }
}

#[cfg(test)]
mod tests;
