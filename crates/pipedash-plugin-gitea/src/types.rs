use chrono::{
    DateTime,
    Utc,
};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct User {
    pub id: i64,
    pub login: String,
    #[serde(default)]
    pub full_name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Organization {
    pub id: i64,
    pub username: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Owner {
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Repository {
    pub name: String,
    pub full_name: String,
    #[serde(default)]
    pub description: String,
    pub owner: Owner,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SearchResults {
    pub data: Vec<Repository>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Workflow {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkflowList {
    pub workflows: Vec<Workflow>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Actor {
    pub login: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Run {
    pub id: i64,
    pub run_number: i64,
    pub status: String,
    #[serde(default)]
    pub conclusion: Option<String>,
    pub path: String,
    #[serde(default)]
    pub event: String,
    #[serde(default)]
    pub display_title: String,
    #[serde(default)]
    pub head_branch: String,
    #[serde(default)]
    pub head_sha: String,
    pub html_url: String,
    #[serde(default)]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub trigger_actor: Option<Actor>,
}

impl Run {
    pub fn workflow_file(&self) -> &str {
        self.path
            .split_once('@')
            .map_or(self.path.as_str(), |(file, _)| file)
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct RunList {
    pub workflow_runs: Vec<Run>,
}
