use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

/// Pipeline status enum
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

/// Available pipeline for selection during provider setup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailablePipeline {
    /// Unique identifier (e.g., "owner/repo" for GitHub, "org/pipeline-slug"
    /// for Buildkite)
    pub id: String,
    /// Display name
    pub name: String,
    /// Description or additional info
    pub description: Option<String>,
    /// Organization/Owner (for filtering)
    pub organization: Option<String>,
    /// Repository name (for filtering)
    pub repository: Option<String>,
}

/// A CI/CD pipeline (workflow)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pipeline {
    /// Unique ID (format:
    /// `provider__{provider_id}__{org}__{repo}__{workflow_id}`)
    pub id: String,
    /// Provider ID from database
    pub provider_id: i64,
    /// Provider type (e.g., "github", "buildkite")
    pub provider_type: String,
    /// Pipeline/workflow name
    pub name: String,
    /// Current status
    pub status: PipelineStatus,
    /// Last run timestamp
    pub last_run: Option<DateTime<Utc>>,
    /// Last updated timestamp
    pub last_updated: DateTime<Utc>,
    /// Repository name (e.g., "owner/repo")
    pub repository: String,
    /// Branch name (optional)
    pub branch: Option<String>,
    /// Workflow file path (optional)
    pub workflow_file: Option<String>,
    /// Provider-specific metadata for custom columns/display
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// A single pipeline run/build
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineRun {
    /// Unique run ID
    pub id: String,
    /// Pipeline ID this run belongs to
    pub pipeline_id: String,
    /// Run number (incremental)
    pub run_number: i64,
    /// Run status
    pub status: PipelineStatus,
    /// When the run started
    pub started_at: DateTime<Utc>,
    /// When the run concluded (if finished)
    pub concluded_at: Option<DateTime<Utc>>,
    /// Duration in seconds
    pub duration_seconds: Option<i64>,
    /// URL to view logs
    pub logs_url: String,
    /// Commit SHA
    pub commit_sha: Option<String>,
    /// Commit message
    pub commit_message: Option<String>,
    /// Branch name
    pub branch: Option<String>,
    /// Actor who triggered the run
    pub actor: Option<String>,
    /// Input parameters used for this run
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<serde_json::Value>,
    /// Provider-specific metadata for custom columns/display
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Parameters for triggering a pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerParams {
    /// Workflow/pipeline ID
    pub workflow_id: String,
    /// Inputs including branch/ref and other parameters
    pub inputs: Option<serde_json::Value>,
}

/// Build agent (for Buildkite, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildAgent {
    /// Agent ID
    pub id: String,
    /// Agent name
    pub name: String,
    /// Hostname
    pub hostname: String,
    /// Agent status (idle, busy, offline)
    pub status: String,
    /// Current job ID (if busy)
    pub job_id: Option<String>,
    /// Last seen timestamp
    pub last_seen: DateTime<Utc>,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Build queue information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildQueue {
    /// Queue name/ID
    pub id: String,
    /// Number of jobs waiting
    pub waiting: usize,
    /// Number of jobs running
    pub running: usize,
    /// Average wait time in seconds
    pub avg_wait_time: Option<i64>,
}

/// Build artifact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildArtifact {
    /// Artifact ID
    pub id: String,
    /// Pipeline run ID
    pub run_id: String,
    /// Artifact filename
    pub filename: String,
    /// File size in bytes
    pub size_bytes: i64,
    /// Download URL
    pub download_url: String,
    /// Content type
    pub content_type: Option<String>,
    /// Created timestamp
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

/// Workflow parameter type
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

/// Workflow parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowParameter {
    /// Parameter name/key
    pub name: String,
    /// Display label
    pub label: Option<String>,
    /// Description
    pub description: Option<String>,
    /// Parameter type and default
    #[serde(flatten)]
    pub param_type: WorkflowParameterType,
    /// Whether this parameter is required
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

/// Permission required by a provider
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Permission {
    /// Permission name (e.g., "repo", "workflow", "read:org")
    pub name: String,
    /// Human-readable description of what this permission allows
    pub description: String,
    /// Whether this permission is strictly required for basic functionality
    pub required: bool,
}

/// Permission check result for a single permission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionCheck {
    /// The permission being checked
    pub permission: Permission,
    /// Whether the token has this permission
    pub granted: bool,
}

/// Overall permission status for a provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionStatus {
    /// Individual permission check results
    pub permissions: Vec<PermissionCheck>,
    /// Whether all required permissions are granted
    pub all_granted: bool,
    /// When these permissions were last checked
    pub checked_at: DateTime<Utc>,
    /// Plugin-specific metadata (e.g., token type, additional context)
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

/// Feature provided by a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    /// Feature identifier (e.g., "trigger_workflow", "view_logs")
    pub id: String,
    /// Human-readable feature name
    pub name: String,
    /// Description of what this feature does
    pub description: String,
    /// Permissions required for this feature to work
    pub required_permissions: Vec<String>,
}

/// Feature availability based on granted permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureAvailability {
    /// The feature being checked
    pub feature: Feature,
    /// Whether this feature is available (all required permissions granted)
    pub available: bool,
    /// List of missing permissions (if not available)
    pub missing_permissions: Vec<String>,
}
