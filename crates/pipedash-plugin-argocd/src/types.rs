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
    pub source: Option<ApplicationSource>,
    pub destination: ApplicationDestination,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Deserialize)]
pub struct ApplicationTree {
    #[serde(default, deserialize_with = "null_as_empty")]
    pub nodes: Vec<ResourceNode>,
}

#[derive(Debug, Deserialize)]
pub struct ResourceNode {
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub namespace: String,
    #[serde(default)]
    pub name: String,
    pub health: Option<HealthStatus>,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub info: Vec<InfoItem>,
}

#[derive(Debug, Deserialize)]
pub struct InfoItem {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct EventList {
    #[serde(default, deserialize_with = "null_as_empty")]
    pub items: Vec<Event>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    #[serde(rename = "type", default)]
    pub event_type: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub count: i64,
    pub first_timestamp: Option<DateTime<Utc>>,
    pub last_timestamp: Option<DateTime<Utc>>,
    pub event_time: Option<DateTime<Utc>>,
}

impl Event {
    pub fn time(&self) -> Option<DateTime<Utc>> {
        self.last_timestamp
            .or(self.event_time)
            .or(self.first_timestamp)
    }
}

#[derive(Debug, Deserialize)]
pub struct PodLogMessage {
    pub result: Option<PodLogEntry>,
    pub error: Option<StreamError>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PodLogEntry {
    #[serde(default)]
    pub content: String,
    pub time_stamp_str: Option<String>,
    #[serde(default)]
    pub last: bool,
}

#[derive(Debug, Deserialize)]
pub struct StreamError {
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub http_code: u16,
}

#[derive(Debug, Deserialize)]
pub struct ResourceManifest {
    pub manifest: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Pod {
    pub metadata: PodMetadata,
    pub spec: PodSpec,
    pub status: PodStatus,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PodMetadata {
    pub uid: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PodSpec {
    pub node_name: Option<String>,
    pub init_containers: Vec<PodContainer>,
    pub containers: Vec<PodContainer>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PodContainer {
    pub name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PodStatus {
    pub phase: String,
    pub reason: Option<String>,
    pub message: Option<String>,
    pub init_container_statuses: Vec<ContainerStatus>,
    pub container_statuses: Vec<ContainerStatus>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ContainerStatus {
    pub name: String,
    pub restart_count: i64,
    pub state: ContainerState,
    pub last_state: ContainerState,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ContainerState {
    pub waiting: Option<ContainerWaiting>,
    pub terminated: Option<ContainerTerminated>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ContainerWaiting {
    pub reason: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ContainerTerminated {
    pub exit_code: i64,
    pub reason: Option<String>,
    pub message: Option<String>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP_OK: &str = r#"{"metadata":{"name":"ok"},"spec":{"source":{"repoURL":"https://github.com/acme/ok","path":"k8s","targetRevision":"main"},"destination":{"server":"https://kubernetes.default.svc","namespace":"ok"},"project":"default"},"status":{"sync":{"status":"Synced"},"health":{"status":"Healthy"}}}"#;

    fn parse_list_with(app: &str) -> Vec<String> {
        let list: ApplicationList =
            serde_json::from_str(&format!(r#"{{"metadata":{{}},"items":[{APP_OK},{app}]}}"#))
                .unwrap();
        list.items.into_iter().map(|a| a.metadata.name).collect()
    }

    #[test]
    fn list_keeps_app_with_destination_name() {
        let app = r#"{"metadata":{"name":"by-name"},"spec":{"source":{"repoURL":"https://github.com/acme/a","path":"k8s","targetRevision":"main"},"destination":{"name":"in-cluster","namespace":"a"},"project":"default"},"status":{"sync":{"status":"Synced"},"health":{"status":"Healthy"}}}"#;
        assert_eq!(parse_list_with(app), ["ok", "by-name"]);
    }

    #[test]
    fn list_keeps_app_without_target_revision_or_namespace() {
        let app = r#"{"metadata":{"name":"head"},"spec":{"source":{"repoURL":"https://github.com/acme/b","path":"k8s"},"destination":{"server":"https://kubernetes.default.svc"},"project":"default"},"status":{"sync":{"status":"Synced"},"health":{"status":"Healthy"}}}"#;
        assert_eq!(parse_list_with(app), ["ok", "head"]);
    }

    #[test]
    fn list_keeps_multi_source_app_with_history() {
        let app = r#"{"metadata":{"name":"multi"},"spec":{"sources":[{"repoURL":"https://github.com/acme/c","path":"k8s","targetRevision":"main"},{"repoURL":"https://charts.acme.dev","chart":"redis","targetRevision":"18.1.0"}],"destination":{"server":"https://kubernetes.default.svc","namespace":"c"},"project":"default"},"status":{"sync":{"status":"Synced","comparedTo":{"source":{"repoURL":""},"sources":[{"repoURL":"https://github.com/acme/c","path":"k8s","targetRevision":"main"}],"destination":{"server":"https://kubernetes.default.svc","namespace":"c"}},"revisions":["abc123","18.1.0"]},"health":{"status":"Healthy"},"history":[{"deployedAt":"2026-10-20T10:00:00Z","id":0,"source":{"repoURL":""},"sources":[{"repoURL":"https://github.com/acme/c","path":"k8s","targetRevision":"main"}],"revisions":["abc123","18.1.0"]}]}}"#;
        assert_eq!(parse_list_with(app), ["ok", "multi"]);
    }

    #[test]
    fn list_keeps_app_awaiting_first_reconcile() {
        let app = r#"{"metadata":{"name":"new"},"spec":{"source":{"repoURL":"https://github.com/acme/d","path":"k8s","targetRevision":"main"},"destination":{"server":"https://kubernetes.default.svc","namespace":"d"},"project":"default"},"status":{"sync":{"status":"","comparedTo":{"source":{"repoURL":""},"destination":{}}},"health":{}}}"#;
        assert_eq!(parse_list_with(app), ["ok", "new"]);
    }

    #[test]
    fn primary_source_prefers_sources_over_empty_source() {
        let spec: ApplicationSpec = serde_json::from_str(
            r#"{"source":{"repoURL":""},"sources":[{"repoURL":"https://github.com/acme/c","path":"k8s","targetRevision":"main"}],"destination":{"server":"https://kubernetes.default.svc"},"project":"default"}"#,
        )
        .unwrap();
        assert_eq!(spec.primary_source().repo_url, "https://github.com/acme/c");
    }
}
