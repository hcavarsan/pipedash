use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct List<T> {
    #[serde(default = "Vec::new")]
    pub items: Vec<T>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Meta {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub namespace: String,
    pub uid: Option<String>,
    pub creation_timestamp: Option<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowTemplate {
    #[serde(default)]
    pub metadata: Meta,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CronWorkflow {
    #[serde(default)]
    pub metadata: Meta,
    #[serde(default)]
    pub spec: CronSpec,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct CronSpec {
    pub schedule: Option<String>,
    #[serde(default)]
    pub schedules: Vec<String>,
    #[serde(default)]
    pub suspend: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Workflow {
    #[serde(default)]
    pub metadata: Meta,
    #[serde(default)]
    pub spec: WorkflowSpec,
    #[serde(default)]
    pub status: WorkflowStatus,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct WorkflowSpec {
    pub shutdown: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkflowStatus {
    pub phase: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub message: Option<String>,
    pub progress: Option<String>,
}
