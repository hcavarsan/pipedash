use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ObjectMeta {
    pub name: String,
    pub namespace: String,
    pub uid: Option<String>,
    #[serde(rename = "creationTimestamp")]
    pub creation_timestamp: Option<String>,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub annotations: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Condition {
    #[serde(rename = "type")]
    pub type_: String,
    pub status: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub message: String,
    #[serde(rename = "lastTransitionTime")]
    pub last_transition_time: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PipelineParam {
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub param_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceDeclaration {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PipelineSpec {
    #[serde(default)]
    pub params: Vec<PipelineParam>,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceDeclaration>,
    #[serde(default)]
    pub tasks: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TektonPipeline {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: ObjectMeta,
    pub spec: PipelineSpec,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ParamValue {
    pub name: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PipelineRef {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkspaceBinding {
    pub name: String,
    #[serde(rename = "emptyDir", skip_serializing_if = "Option::is_none")]
    pub empty_dir: Option<serde_json::Value>,
    #[serde(
        rename = "persistentVolumeClaim",
        skip_serializing_if = "Option::is_none"
    )]
    pub persistent_volume_claim: Option<PvcWorkspaceBinding>,
    #[serde(rename = "configMap", skip_serializing_if = "Option::is_none")]
    pub config_map: Option<ConfigMapWorkspaceBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<SecretWorkspaceBinding>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PvcWorkspaceBinding {
    #[serde(rename = "claimName")]
    pub claim_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigMapWorkspaceBinding {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SecretWorkspaceBinding {
    #[serde(rename = "secretName")]
    pub secret_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskRunTemplate {
    #[serde(rename = "serviceAccountName", skip_serializing_if = "Option::is_none")]
    pub service_account_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PipelineRunSpec {
    #[serde(rename = "pipelineRef", skip_serializing_if = "Option::is_none")]
    pub pipeline_ref: Option<PipelineRef>,
    #[serde(default)]
    pub params: Vec<ParamValue>,
    #[serde(default)]
    pub workspaces: Vec<WorkspaceBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    #[serde(rename = "taskRunTemplate", skip_serializing_if = "Option::is_none")]
    pub task_run_template: Option<TaskRunTemplate>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StepTerminated {
    #[serde(rename = "exitCode", default)]
    pub exit_code: i32,
    #[serde(rename = "startedAt", default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(
        rename = "finishedAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StepRunning {
    #[serde(rename = "startedAt", default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StepState {
    pub name: String,
    #[serde(default)]
    pub container: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminated: Option<StepTerminated>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running: Option<StepRunning>,
    #[serde(
        rename = "terminationReason",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub termination_reason: Option<String>,
}

impl StepState {
    pub fn has_started(&self) -> bool {
        self.terminated.is_some() || self.running.is_some()
    }

    pub fn started_at(&self) -> Option<DateTime<Utc>> {
        let stamp = match (&self.terminated, &self.running) {
            (Some(terminated), _) => &terminated.started_at,
            (None, Some(running)) => &running.started_at,
            (None, None) => return None,
        };
        parse_timestamp(stamp)
    }

    pub fn finished_at(&self) -> Option<DateTime<Utc>> {
        parse_timestamp(&self.terminated.as_ref()?.finished_at)
    }

    pub fn container_name(&self) -> String {
        if self.container.is_empty() {
            format!("step-{}", self.name)
        } else {
            self.container.clone()
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TaskRunStatusFields {
    #[serde(rename = "startTime", skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    #[serde(rename = "completionTime", skip_serializing_if = "Option::is_none")]
    pub completion_time: Option<String>,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    #[serde(rename = "podName", default, skip_serializing_if = "Option::is_none")]
    pub pod_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<StepState>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TektonTaskRun {
    pub metadata: ObjectMeta,
    #[serde(default)]
    pub status: TaskRunStatusFields,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskRunStatus {
    #[serde(rename = "pipelineTaskName")]
    pub pipeline_task_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TaskRunStatusFields>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChildReference {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub name: String,
    #[serde(rename = "pipelineTaskName")]
    pub pipeline_task_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PipelineRunStatus {
    #[serde(default)]
    pub conditions: Vec<Condition>,
    #[serde(rename = "startTime", skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    #[serde(rename = "completionTime", skip_serializing_if = "Option::is_none")]
    pub completion_time: Option<String>,
    #[serde(rename = "taskRuns", default)]
    pub task_runs: HashMap<String, TaskRunStatus>,
    #[serde(rename = "childReferences", default)]
    pub child_references: Vec<ChildReference>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TektonPipelineRun {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: String,
    pub metadata: ObjectMeta,
    pub spec: PipelineRunSpec,
    #[serde(default)]
    pub status: PipelineRunStatus,
}

#[derive(Debug, Deserialize)]
pub struct PipelineList {
    pub items: Vec<TektonPipeline>,
}

#[derive(Debug, Deserialize)]
pub struct PipelineRunList {
    pub items: Vec<TektonPipelineRun>,
}

#[derive(Debug, Deserialize)]
pub struct TaskRunList {
    pub items: Vec<TektonTaskRun>,
}

pub fn parse_timestamp(timestamp: &Option<String>) -> Option<DateTime<Utc>> {
    timestamp.as_ref()?.parse::<DateTime<Utc>>().ok()
}
