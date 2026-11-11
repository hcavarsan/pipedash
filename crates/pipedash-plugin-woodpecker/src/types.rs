use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Repo {
    pub id: i64,
    pub owner: String,
    pub name: String,
    pub full_name: String,
    #[serde(default)]
    pub default_branch: String,
    #[serde(default)]
    pub config_file: String,
    #[serde(default)]
    pub forge_url: Option<String>,
    #[serde(default)]
    pub clone_url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Pipeline {
    pub id: i64,
    pub number: i64,
    pub status: String,
    #[serde(default)]
    pub event: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub commit: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub started: i64,
    #[serde(default)]
    pub finished: i64,
    #[serde(default)]
    pub workflows: Option<Vec<Workflow>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Workflow {
    pub id: i64,
    pub name: String,
    pub state: String,
    #[serde(default)]
    pub started: i64,
    #[serde(default)]
    pub finished: i64,
    #[serde(default)]
    pub children: Option<Vec<Step>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Step {
    pub id: i64,
    pub pid: i64,
    pub name: String,
    pub state: String,
    #[serde(default)]
    pub started: i64,
    #[serde(default)]
    pub finished: i64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LogEntry {
    #[serde(default)]
    pub data: Option<String>,
}
