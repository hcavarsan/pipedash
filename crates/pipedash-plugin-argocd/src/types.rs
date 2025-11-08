use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationList {
    pub items: Vec<Application>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Application {
    pub metadata: ApplicationMetadata,
    pub spec: ApplicationSpec,
    pub status: ApplicationStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationMetadata {
    pub name: String,
    pub namespace: Option<String>,
    #[serde(rename = "creationTimestamp")]
    pub creation_timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationSpec {
    pub source: ApplicationSource,
    pub destination: ApplicationDestination,
    pub project: String,
    #[serde(rename = "syncPolicy")]
    pub sync_policy: Option<SyncPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationSource {
    #[serde(rename = "repoURL")]
    pub repo_url: String,
    pub path: Option<String>,
    #[serde(rename = "targetRevision")]
    pub target_revision: String,
    pub chart: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationDestination {
    pub server: String,
    pub namespace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPolicy {
    pub automated: Option<AutomatedSyncPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomatedSyncPolicy {
    pub prune: Option<bool>,
    #[serde(rename = "selfHeal")]
    pub self_heal: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationStatus {
    pub sync: SyncStatus,
    pub health: HealthStatus,
    #[serde(rename = "operationState")]
    pub operation_state: Option<OperationState>,
    pub history: Option<Vec<RevisionHistory>>,
    #[serde(default)]
    pub resources: Vec<ResourceStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub status: String,
    pub revision: Option<String>,
    #[serde(rename = "comparedTo")]
    pub compared_to: Option<ComparedTo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparedTo {
    pub source: ApplicationSource,
    pub destination: ApplicationDestination,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationState {
    pub phase: String,
    #[serde(rename = "startedAt")]
    pub started_at: DateTime<Utc>,
    #[serde(rename = "finishedAt")]
    pub finished_at: Option<DateTime<Utc>>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionHistory {
    pub revision: String,
    #[serde(rename = "deployedAt")]
    pub deployed_at: DateTime<Utc>,
    pub id: i64,
    pub source: Option<ApplicationSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStatus {
    pub group: Option<String>,
    pub version: Option<String>,
    pub kind: String,
    pub namespace: Option<String>,
    pub name: String,
    pub status: Option<String>,
    pub health: Option<HealthStatus>,
}

/// Sync operation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    pub revision: Option<String>,
    pub prune: Option<bool>,
    #[serde(rename = "dryRun")]
    pub dry_run: Option<bool>,
    pub strategy: Option<SyncStrategy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStrategy {
    pub hook: Option<SyncStrategyHook>,
    pub apply: Option<SyncStrategyApply>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStrategyHook {
    pub force: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStrategyApply {
    pub force: Option<bool>,
}
