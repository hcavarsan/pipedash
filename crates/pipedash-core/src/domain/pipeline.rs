use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
pub use pipedash_plugin_api::{
    PaginatedAvailablePipelines,
    PaginationParams,
    ResourceState,
};
use pipedash_plugin_api::{
    PipelineActionState,
    RunActionState,
    RunDisplay,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PipelineStatus {
    Success,
    Failed,
    Degraded,
    Running,
    Pending,
    Cancelled,
    Skipped,
}

impl PipelineStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            PipelineStatus::Success
                | PipelineStatus::Failed
                | PipelineStatus::Degraded
                | PipelineStatus::Cancelled
                | PipelineStatus::Skipped
        )
    }

    pub fn needs_attention(&self) -> bool {
        matches!(self, PipelineStatus::Failed | PipelineStatus::Degraded)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PipelineStatus::Success => "success",
            PipelineStatus::Failed => "failed",
            PipelineStatus::Degraded => "degraded",
            PipelineStatus::Running => "running",
            PipelineStatus::Pending => "pending",
            PipelineStatus::Cancelled => "cancelled",
            PipelineStatus::Skipped => "skipped",
        }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_state: Option<ResourceState>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overdue_run: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typical_duration_seconds: Option<i64>,
    #[serde(default)]
    pub actions: PipelineActionState,
}

impl Pipeline {
    pub fn finished_since(&self, status: &PipelineStatus, last_run: Option<DateTime<Utc>>) -> bool {
        matches!(
            self.status,
            PipelineStatus::Success
                | PipelineStatus::Failed
                | PipelineStatus::Cancelled
                | PipelineStatus::Skipped
        ) && (!status.is_terminal() || last_run != self.last_run)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipelineRun {
    pub id: String,
    pub pipeline_id: String,
    pub run_number: i64,
    pub status: PipelineStatus,
    pub started_at: DateTime<Utc>,
    pub concluded_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i64>,
    pub logs_url: String,
    pub commit_sha: Option<String>,
    pub commit_message: Option<String>,
    pub branch: Option<String>,
    pub actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub actions: RunActionState,
    #[serde(default)]
    pub display: RunDisplay,
    #[serde(default)]
    pub pace: RunPace,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunPace {
    pub live: bool,
    pub typical_duration_seconds: Option<i64>,
    pub past_usual: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerParams {
    pub workflow_id: String,
    pub inputs: Option<serde_json::Value>,
}
