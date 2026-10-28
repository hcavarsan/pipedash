use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct VersionResponse {
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StackListItem {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub template: bool,
    #[serde(default)]
    pub info: StackInfo,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct StackInfo {
    #[serde(default)]
    pub server_id: String,
    #[serde(default)]
    pub server_name: String,
    #[serde(default)]
    pub swarm_name: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub repo_link: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub services: Vec<StackService>,
    #[serde(default)]
    pub project_missing: bool,
    #[serde(default)]
    pub missing_files: Vec<String>,
    #[serde(default)]
    pub deployed_hash: Option<String>,
    #[serde(default)]
    pub latest_hash: Option<String>,
}

impl StackInfo {
    pub fn host(&self) -> &str {
        [&self.server_name, &self.swarm_name, &self.server_id]
            .into_iter()
            .find(|name| !name.is_empty())
            .map_or("", String::as_str)
    }

    pub fn update_available(&self) -> bool {
        self.services.iter().any(|service| service.update_available)
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct StackService {
    #[serde(default)]
    pub update_available: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListUpdatesResponse {
    #[serde(default)]
    pub updates: Vec<UpdateListItem>,
    #[serde(default)]
    pub next_page: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateListItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub operation: String,
    #[serde(default)]
    pub start_ts: i64,
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub operator: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub target: UpdateTarget,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct UpdateTarget {
    #[serde(default)]
    pub id: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum MongoId {
    Object {
        #[serde(rename = "$oid")]
        oid: String,
    },
    Plain(String),
}

impl MongoId {
    pub fn into_string(self) -> String {
        match self {
            Self::Object { oid } => oid,
            Self::Plain(id) => id,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct Update {
    #[serde(default, rename = "_id")]
    pub id: Option<MongoId>,
    #[serde(default)]
    pub start_ts: i64,
    #[serde(default)]
    pub end_ts: Option<i64>,
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub commit_hash: String,
}
