use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PipelineStatus {
    Success,
    Failed,
    Running,
    Pending,
    Cancelled,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailablePipeline {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub organization: Option<String>,
    pub repository: Option<String>,
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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
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
    pub commit_sha: Option<String>,
    pub commit_message: Option<String>,
    pub branch: Option<String>,
    pub actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerParams {
    pub workflow_id: String,
    pub inputs: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildAgent {
    pub id: String,
    pub name: String,
    pub hostname: String,
    pub status: String,
    pub job_id: Option<String>,
    pub last_seen: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildQueue {
    pub id: String,
    pub waiting: usize,
    pub running: usize,
    pub avg_wait_time: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildArtifact {
    pub id: String,
    pub run_id: String,
    pub filename: String,
    pub size_bytes: i64,
    pub download_url: String,
    pub content_type: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum WorkflowParameterType {
    String {
        #[serde(default)]
        default: Option<String>,
    },
    Boolean {
        #[serde(default)]
        default: bool,
    },
    Choice {
        options: Vec<String>,
        #[serde(default)]
        default: Option<String>,
    },
    Number {
        #[serde(default)]
        default: Option<f64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowParameter {
    pub name: String,
    pub label: Option<String>,
    pub description: Option<String>,
    #[serde(flatten)]
    pub param_type: WorkflowParameterType,
    #[serde(default)]
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginationParams {
    pub page: usize,
    pub page_size: usize,
}

impl PaginationParams {
    pub fn validate(&self) -> Result<(), String> {
        if self.page == 0 {
            return Err("Page must be >= 1".to_string());
        }
        if self.page_size == 0 {
            return Err("Page size must be >= 1".to_string());
        }
        if self.page_size > 200 {
            return Err("Page size must be <= 200".to_string());
        }
        Ok(())
    }

    pub fn calculate_offset(&self) -> Result<usize, String> {
        self.page
            .checked_sub(1)
            .and_then(|p| p.checked_mul(self.page_size))
            .ok_or_else(|| "Pagination offset overflow".to_string())
    }
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            page: 1,
            page_size: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub page: usize,
    pub page_size: usize,
    pub total_count: usize,
    pub total_pages: usize,
    pub has_more: bool,
}

impl<T> PaginatedResponse<T> {
    pub fn new(items: Vec<T>, page: usize, page_size: usize, total_count: usize) -> Self {
        let items_count = items.len();
        let total_pages = if page_size > 0 {
            total_count.div_ceil(page_size)
        } else {
            1
        };
        let has_more = items_count == page_size;

        Self {
            items,
            page,
            page_size,
            total_count,
            total_pages,
            has_more,
        }
    }

    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            page: 1,
            page_size: 100,
            total_count: 0,
            total_pages: 0,
            has_more: false,
        }
    }
}

pub type PaginatedAvailablePipelines = PaginatedResponse<AvailablePipeline>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Permission {
    pub name: String,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionCheck {
    pub permission: Permission,
    pub granted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionStatus {
    pub permissions: Vec<PermissionCheck>,
    pub all_granted: bool,
    pub checked_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    pub id: String,
    pub name: String,
    pub description: String,
    pub required_permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureAvailability {
    pub feature: Feature,
    pub available: bool,
    pub missing_permissions: Vec<String>,
}
