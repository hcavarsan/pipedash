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
        self.source
            .as_ref()
            .or(self.sources.first())
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
    pub revision: Option<String>,
    #[serde(default)]
    pub revisions: Vec<String>,
    #[serde(rename = "deployedAt")]
    pub deployed_at: DateTime<Utc>,
    pub id: i64,
    pub source: Option<ApplicationSource>,
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
}
