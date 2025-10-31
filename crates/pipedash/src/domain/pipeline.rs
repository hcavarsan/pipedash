use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailablePipeline {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub organization: Option<String>,
    pub repository: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PipelineStatus {
    Success,
    Failed,
    Running,
    Pending,
    Cancelled,
    Skipped,
}

impl PipelineStatus {
    #[allow(dead_code)]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            PipelineStatus::Success
                | PipelineStatus::Failed
                | PipelineStatus::Cancelled
                | PipelineStatus::Skipped
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: String,
    pub provider_id: i64,
    pub provider_type: String,
    pub name: String,
    pub status: PipelineStatus,
    pub last_run: Option<DateTime<Utc>>,
    pub last_updated: DateTime<Utc>,
    pub repository: String,
    pub branch: Option<String>,
    pub workflow_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineRun {
    pub id: String,
    pub pipeline_id: String,
    pub run_number: i64,
    pub status: PipelineStatus,
    pub started_at: DateTime<Utc>,
    pub concluded_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i64>,
    pub logs_url: String,
    pub commit_sha: String,
    pub commit_message: Option<String>,
    pub branch: String,
    pub actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerParams {
    pub workflow_id: String,
    pub inputs: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedRunHistory {
    pub runs: Vec<PipelineRun>,
    pub total_count: usize,
    pub has_more: bool,
    pub is_complete: bool,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
}
