use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Deserializer,
};

#[derive(Debug, Deserialize)]
pub(crate) struct Application {
    pub uuid: String,
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub git_repository: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
    #[serde(default)]
    pub build_pack: Option<String>,
    #[serde(default)]
    pub docker_registry_image_name: Option<String>,
    #[serde(default)]
    pub docker_registry_image_tag: Option<String>,
    #[serde(default)]
    pub fqdn: Option<String>,
    #[serde(default)]
    pub environment_id: Option<i64>,
    #[serde(default)]
    pub destination: Option<Destination>,
}

impl Application {
    pub fn server_name(&self) -> Option<&str> {
        self.destination
            .as_ref()
            .and_then(|destination| destination.server.as_ref())
            .and_then(|server| server.name.as_deref())
            .filter(|name| !name.is_empty())
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct Destination {
    #[serde(default)]
    pub server: Option<Server>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Server {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Project {
    pub uuid: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ProjectDetail {
    #[serde(default)]
    pub environments: Vec<Environment>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Environment {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeploymentPage {
    #[serde(default)]
    pub deployments: Vec<Deployment>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Deployment {
    pub id: i64,
    pub deployment_uuid: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub commit_message: Option<String>,
    #[serde(default)]
    pub deployment_url: Option<String>,
    #[serde(default)]
    pub server_name: Option<String>,
    #[serde(default)]
    pub pull_request_id: i64,
    #[serde(default)]
    pub is_webhook: bool,
    #[serde(default)]
    pub is_api: bool,
    #[serde(default)]
    pub rollback: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeploymentDetail {
    pub id: i64,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub deployment_url: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub logs: Option<Option<String>>,
}

fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(d).map(Some)
}

#[derive(Debug, Deserialize)]
pub(crate) struct LogEntry {
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default, rename = "type")]
    pub stream: Option<String>,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default)]
    pub batch: Option<i64>,
    #[serde(default)]
    pub order: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DeployResponse {
    #[serde(default)]
    pub deployments: Vec<QueuedDeployment>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct QueuedDeployment {
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub deployment_uuid: Option<String>,
}
