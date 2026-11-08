use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Pipeline {
    pub name: String,
    pub repository: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Build {
    pub id: String,
    pub number: i64,
    pub state: String,
    pub web_url: String,
    #[serde(default)]
    pub branch: String,
    pub message: Option<String>,
    #[serde(default)]
    pub commit: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub author: Option<Author>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Author {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BuildJobs {
    #[serde(default)]
    pub jobs: Vec<Job>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Job {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub name: Option<String>,
    pub command: Option<String>,
    pub step_key: Option<String>,
    pub state: Option<String>,
    pub web_url: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub retried: Option<bool>,
    pub parallel_group_index: Option<i64>,
    pub parallel_group_total: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Organization {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}
