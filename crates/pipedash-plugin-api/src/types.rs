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
