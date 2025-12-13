use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub uuid: String,
    pub display_name: String,
    pub nickname: Option<String>,
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub uuid: String,
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    pub uuid: String,
    pub name: String,
    pub full_name: String,
    pub slug: String,
    #[serde(default)]
    pub description: Option<String>,
    pub workspace: WorkspaceRef,
    pub links: RepositoryLinks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceRef {
    pub uuid: String,
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryLinks {
    pub html: Link,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub href: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub uuid: String,
    pub build_number: i64,
    pub state: PipelineState,
    pub target: PipelineTarget,
    pub created_on: DateTime<Utc>,
    #[serde(default)]
    pub completed_on: Option<DateTime<Utc>>,
    #[serde(default)]
    pub duration_in_seconds: Option<i64>,
    #[serde(default)]
    pub creator: Option<User>,
    pub links: PipelineLinks,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineState {
    pub name: String,
    #[serde(default)]
    pub result: Option<PipelineResult>,
    /// Present when IN_PROGRESS but paused for manual approval
    #[serde(default)]
    pub stage: Option<PipelineStateStage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStateStage {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineResult {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTarget {
    #[serde(rename = "type")]
    pub target_type: String,
    #[serde(default)]
    pub ref_name: Option<String>,
    #[serde(default)]
    pub ref_type: Option<String>,
    #[serde(default)]
    pub commit: Option<PipelineCommit>,
    #[serde(default)]
    pub selector: Option<PipelineSelector>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineCommit {
    pub hash: String,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineSelector {
    #[serde(rename = "type")]
    pub selector_type: String,
    #[serde(default)]
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineLinks {
    #[serde(default, rename = "self")]
    pub self_link: Option<Link>,
    #[serde(default)]
    pub html: Option<Link>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStep {
    pub uuid: String,
    #[serde(default)]
    pub name: Option<String>,
    pub state: PipelineStepState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStepState {
    pub name: String,
    /// Present when PENDING and waiting for manual trigger
    #[serde(default)]
    pub stage: Option<PipelineStepStage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStepStage {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub values: Vec<T>,
    #[serde(default)]
    pub page: Option<usize>,
    pub pagelen: usize,
    #[serde(default)]
    pub size: Option<usize>,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub previous: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerPipelineRequest {
    pub target: TriggerTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerTarget {
    #[serde(rename = "type")]
    pub target_type: String,
    pub ref_name: String,
    pub ref_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selector: Option<TriggerSelector>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerSelector {
    #[serde(rename = "type")]
    pub selector_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}
