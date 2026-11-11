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
    pub id: i64,
    pub username: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub name_with_namespace: String,
    pub description: Option<String>,
    pub web_url: String,
    #[serde(default)]
    pub ci_config_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: i64,
    #[serde(default)]
    pub project_id: Option<i64>,
    pub status: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub sha: String,
    pub web_url: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration: Option<i64>,
    pub user: Option<PipelineUser>,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Job {
    pub id: i64,
    pub name: String,
    pub stage: String,
    pub status: String,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub web_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineUser {
    pub username: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerPipelineRequest {
    #[serde(rename = "ref")]
    pub ref_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variables: Option<Vec<PipelineVariable>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineVariable {
    pub key: String,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variable_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Deployment {
    pub environment: DeploymentEnvironment,
    #[serde(default)]
    pub deployable: Option<Deployable>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeploymentEnvironment {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Deployable {
    #[serde(default)]
    pub pipeline: Option<DeployablePipeline>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeployablePipeline {
    pub id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Environment {
    pub name: String,
    #[serde(default)]
    pub auto_stop_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Commit {
    pub id: String,
    #[serde(default)]
    pub message: String,
    pub authored_date: DateTime<Utc>,
    pub committed_date: DateTime<Utc>,
    #[serde(default)]
    pub parent_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommitRef {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Compare {
    #[serde(default)]
    pub commits: Vec<CommitRef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileDiff {
    pub new_path: String,
    #[serde(default)]
    pub diff: String,
}
