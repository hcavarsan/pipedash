use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationList {
    pub items: Vec<Application>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub url: String,
    pub dex_config: Option<DexConfig>,
    pub oidc_config: Option<OidcConfig>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct DexConfig {
    pub connectors: Vec<Connector>,
    #[serde(rename = "dexAuthConnectorID")]
    pub auth_connector_id: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Connector {
    pub name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct OidcConfig {
    pub name: String,
    pub issuer: String,
    #[serde(rename = "clientID")]
    pub client_id: String,
    #[serde(rename = "cliClientID")]
    pub cli_client_id: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Application {
    pub metadata: ApplicationMetadata,
    pub spec: ApplicationSpec,
    #[serde(default)]
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
    pub source: Option<ApplicationSource>,
    #[serde(default)]
    pub sources: Vec<ApplicationSource>,
    pub destination: ApplicationDestination,
    pub project: String,
    #[serde(rename = "syncPolicy")]
    pub sync_policy: Option<SyncPolicy>,
}

impl ApplicationSpec {
    pub fn primary_source(&self) -> &ApplicationSource {
        static NO_SOURCE: ApplicationSource = ApplicationSource {
            repo_url: String::new(),
            path: None,
            target_revision: None,
            chart: None,
        };
        self.sources
            .first()
            .or(self.source.as_ref())
            .unwrap_or(&NO_SOURCE)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationSource {
    #[serde(rename = "repoURL")]
    pub repo_url: String,
    pub path: Option<String>,
    #[serde(rename = "targetRevision")]
    pub target_revision: Option<String>,
    pub chart: Option<String>,
}

impl ApplicationSource {
    pub fn kind(&self) -> &'static str {
        match &self.path {
            _ if self.chart.is_some() => "Helm",
            Some(path)
                if path.to_lowercase().contains("kustomize")
                    || path.ends_with("kustomization.yaml") =>
            {
                "Kustomize"
            }
            _ => "Plain",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationDestination {
    pub server: Option<String>,
    pub name: Option<String>,
    pub namespace: Option<String>,
}

impl ApplicationDestination {
    pub fn cluster(&self) -> &str {
        self.server
            .as_deref()
            .or(self.name.as_deref())
            .unwrap_or_default()
    }
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplicationStatus {
    pub sync: SyncStatus,
    pub health: HealthStatus,
    #[serde(rename = "operationState")]
    pub operation_state: Option<OperationState>,
    pub history: Option<Vec<RevisionHistory>>,
    pub resources: Vec<ResourceStatus>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncStatus {
    pub status: String,
    pub revision: Option<String>,
    #[serde(rename = "comparedTo")]
    pub compared_to: Option<ComparedTo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparedTo {
    pub source: Option<ApplicationSource>,
    pub destination: ApplicationDestination,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HealthStatus {
    #[serde(default)]
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
    #[serde(default)]
    pub operation: Operation,
    #[serde(rename = "syncResult")]
    pub sync_result: Option<SyncOperationResult>,
    #[serde(rename = "retryCount", default)]
    pub retry_count: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Operation {
    pub sync: Option<SyncOperation>,
    #[serde(rename = "initiatedBy", default)]
    pub initiated_by: OperationInitiator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncOperation {
    pub revision: Option<String>,
    #[serde(default)]
    pub prune: bool,
    #[serde(rename = "dryRun", default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OperationInitiator {
    pub username: Option<String>,
    #[serde(default)]
    pub automated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncOperationResult {
    #[serde(default)]
    pub resources: Vec<ResourceResult>,
    pub revision: Option<String>,
    #[serde(default)]
    pub revisions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceResult {
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub namespace: String,
    #[serde(default)]
    pub name: String,
    pub status: Option<String>,
    pub message: Option<String>,
    pub hook_type: Option<String>,
    pub hook_phase: Option<String>,
    pub sync_phase: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevisionHistory {
    pub revision: Option<String>,
    #[serde(default)]
    pub revisions: Vec<String>,
    #[serde(rename = "deployedAt")]
    pub deployed_at: DateTime<Utc>,
    #[serde(rename = "deployStartedAt")]
    pub deploy_started_at: Option<DateTime<Utc>>,
    pub id: i64,
    pub source: Option<ApplicationSource>,
    #[serde(rename = "initiatedBy", default)]
    pub initiated_by: OperationInitiator,
}

impl RevisionHistory {
    pub fn primary_revision(&self) -> Option<&str> {
        self.revision
            .as_deref()
            .or(self.revisions.first().map(String::as_str))
    }
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

fn null_as_empty<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

pub(crate) mod logs;

#[cfg(test)]
mod tests;
