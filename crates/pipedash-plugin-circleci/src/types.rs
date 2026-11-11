use chrono::{
    DateTime,
    Utc,
};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Project {
    pub slug: String,
    pub id: Option<String>,
    pub name: String,
    pub organization: String,
    pub repository_url: String,
    pub default_branch: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CirclePipeline {
    pub id: String,
    pub number: i64,
    pub state: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub vcs: Option<Vcs>,
    pub trigger: Option<Trigger>,
    #[serde(default)]
    pub errors: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Vcs {
    pub branch: Option<String>,
    pub revision: Option<String>,
    pub target_repository_url: Option<String>,
    pub commit: Option<Commit>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Commit {
    pub subject: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Trigger {
    pub actor: Option<Actor>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Actor {
    pub login: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub status: String,
    pub tag: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub stopped_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
pub(crate) struct Page<T> {
    pub items: Vec<T>,
    pub next_page_token: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ProjectResponse {
    pub slug: String,
    pub id: Option<String>,
    pub name: String,
    pub organization_name: String,
    pub vcs_info: Option<ProjectVcs>,
}

#[derive(Deserialize)]
pub(crate) struct ProjectVcs {
    pub vcs_url: String,
    pub default_branch: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct FollowedProject {
    pub vcs_type: Option<String>,
    pub username: String,
    pub reponame: String,
    pub vcs_url: String,
    pub default_branch: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Job {
    pub id: Option<String>,
    pub name: String,
    pub job_number: Option<i64>,
    pub status: String,
    #[serde(rename = "type", default)]
    pub job_type: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub stopped_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct JobSteps {
    #[serde(default)]
    pub steps: Vec<Step>,
}

#[derive(Debug, Deserialize)]
pub struct Step {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub actions: Vec<Action>,
}

#[derive(Debug, Deserialize)]
pub struct Action {
    #[serde(default)]
    pub index: i64,
    pub status: Option<String>,
    pub failed: Option<bool>,
    pub output_url: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct OutputMessage {
    #[serde(default)]
    pub message: String,
    pub time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PipelineDefinition {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PipelineConfig {
    pub source: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PipelineCreation {
    pub number: Option<i64>,
    pub message: Option<String>,
}
