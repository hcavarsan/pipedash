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
    Degraded,
    Running,
    Pending,
    Cancelled,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewDecision {
    Approve,
    Reject,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailablePipeline {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub organization: Option<String>,
    pub repository: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceState {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ResourceState {
    pub fn new(label: impl Into<String>, message: Option<&str>) -> Self {
        Self {
            label: label.into(),
            message: message
                .map(str::trim)
                .filter(|message| !message.is_empty())
                .map(str::to_string),
        }
    }
}

pub const READ_ONLY_METADATA_KEY: &str = "read_only";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    pub id: String,
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
pub struct Organization {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PipelineCatalog {
    pub organizations: Vec<Organization>,
    pub pipelines: Vec<AvailablePipeline>,
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
    #[serde(default)]
    pub credential_label: Option<String>,
}

impl PermissionStatus {
    pub fn evaluate(
        permissions: &[Permission], token_type: &str, mut granted: impl FnMut(&Permission) -> bool,
    ) -> Self {
        let permissions: Vec<_> = permissions
            .iter()
            .map(|permission| PermissionCheck {
                granted: granted(permission),
                permission: permission.clone(),
            })
            .collect();
        let all_granted = permissions
            .iter()
            .filter(|check| check.permission.required)
            .all(|check| check.granted);

        Self {
            permissions,
            all_granted,
            checked_at: Utc::now(),
            metadata: HashMap::from([("token_type".to_string(), token_type.to_string())]),
            credential_label: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimit {
    pub limit: u64,
    pub remaining: u64,
    pub reset_at: DateTime<Utc>,
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

#[cfg(test)]
mod tests;
