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
