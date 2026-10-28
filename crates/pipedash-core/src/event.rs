use async_trait::async_trait;
use serde::{
    Deserialize,
    Serialize,
};

use crate::domain::{
    provider::ProviderSummary,
    HealthLevel,
    Pipeline,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertLinkKind {
    Incident,
    StatusPage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertLink {
    pub url: String,
    pub kind: AlertLinkKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CacheInvalidationReason {
    Fetch,
    ProviderChange,
    ManualRefresh,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum CoreEvent {
    ProvidersChanged,

    PipelineViewsChanged,

    PipelinesUpdated {
        pipelines: Vec<Pipeline>,
        provider_id: i64,
        timestamp: i64,
    },

    PipelineStatusChanged {
        pipeline_ids: Vec<String>,
    },

    PipelinesFailed {
        pipelines: Vec<Pipeline>,
    },

    RunTriggered {
        workflow_id: String,
    },

    RunCancelled {
        pipeline_id: String,
    },

    MetricsGenerated {
        pipeline_id: String,
    },

    MetricsGlobalConfigChanged,

    MetricsConfigChanged {
        pipeline_id: String,
    },

    MetricsFlushed {
        pipeline_id: Option<String>,
    },

    ProviderStatusUpdated {
        provider: ProviderSummary,
    },

    ProviderHealthAlert {
        alert_id: String,
        #[serde(default)]
        alert_key: String,
        notification_key: String,
        provider_ids: Vec<i64>,
        level: HealthLevel,
        title: String,
        body: String,
        link: Option<AlertLink>,
        recovered: bool,
    },

    MigrationProgress {
        step: String,
        step_index: usize,
        total_steps: usize,
        message: String,
    },

    MigrationComplete {
        success: bool,
        summary: String,
    },

    ProviderAdded {
        provider: ProviderSummary,
        timestamp: i64,
    },

    ProviderUpdated {
        provider: ProviderSummary,
        timestamp: i64,
    },

    ProviderRemoved {
        provider: ProviderSummary,
        timestamp: i64,
    },

    PipelineCacheInvalidated {
        provider_id: Option<i64>,
        reason: CacheInvalidationReason,
    },

    RunHistoryCacheInvalidated {
        pipeline_id: Option<String>,
    },

    VaultUnlocked,

    VaultLocked,

    RunLogProgress {
        pipeline_id: String,
        run_number: i64,
        node_id: String,
        received_bytes: u64,
    },

    RunLogReady {
        pipeline_id: String,
        run_number: i64,
        node_id: String,
        revision: i64,
    },

    RunLogFailed {
        pipeline_id: String,
        run_number: i64,
        node_id: String,
        message: String,
    },
}

impl CoreEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            CoreEvent::ProvidersChanged => "providers-changed",
            CoreEvent::PipelineViewsChanged => "pipeline-views-changed",
            CoreEvent::PipelinesUpdated { .. } => "pipelines-updated",
            CoreEvent::PipelineStatusChanged { .. } => "pipeline-status-changed",
            CoreEvent::PipelinesFailed { .. } => "pipelines-failed",
            CoreEvent::RunTriggered { .. } => "run-triggered",
            CoreEvent::RunCancelled { .. } => "run-cancelled",
            CoreEvent::MetricsGenerated { .. } => "metrics-generated",
            CoreEvent::MetricsGlobalConfigChanged => "metrics-global-config-changed",
            CoreEvent::MetricsConfigChanged { .. } => "metrics-config-changed",
            CoreEvent::MetricsFlushed { .. } => "metrics-flushed",
            CoreEvent::ProviderStatusUpdated { .. } => "provider-status-updated",
            CoreEvent::ProviderHealthAlert { .. } => "provider-health-alert",
            CoreEvent::MigrationProgress { .. } => "migration-progress",
            CoreEvent::MigrationComplete { .. } => "migration-complete",
            CoreEvent::ProviderAdded { .. } => "provider-added",
            CoreEvent::ProviderUpdated { .. } => "provider-updated",
            CoreEvent::ProviderRemoved { .. } => "provider-removed",
            CoreEvent::PipelineCacheInvalidated { .. } => "pipeline-cache-invalidated",
            CoreEvent::RunHistoryCacheInvalidated { .. } => "run-history-cache-invalidated",
            CoreEvent::VaultUnlocked => "vault-unlocked",
            CoreEvent::VaultLocked => "vault-locked",
            CoreEvent::RunLogProgress { .. } => "run-log-progress",
            CoreEvent::RunLogReady { .. } => "run-log-ready",
            CoreEvent::RunLogFailed { .. } => "run-log-failed",
        }
    }

    pub fn to_json_payload(&self) -> serde_json::Value {
        match self {
            CoreEvent::ProvidersChanged => serde_json::json!({}),
            CoreEvent::PipelineViewsChanged => serde_json::json!({}),
            CoreEvent::PipelinesUpdated {
                pipelines,
                provider_id,
                timestamp,
            } => serde_json::json!({
                "pipelines": pipelines,
                "providerId": provider_id,
                "timestamp": timestamp,
            }),
            CoreEvent::PipelineStatusChanged { pipeline_ids } => serde_json::json!({
                "pipeline_ids": pipeline_ids,
            }),
            CoreEvent::PipelinesFailed { pipelines } => serde_json::json!({
                "pipelines": pipelines,
            }),
            CoreEvent::RunTriggered { workflow_id } => serde_json::json!(workflow_id),
            CoreEvent::RunCancelled { pipeline_id } => serde_json::json!(pipeline_id),
            CoreEvent::MetricsGenerated { pipeline_id } => serde_json::json!(pipeline_id),
            CoreEvent::MetricsGlobalConfigChanged => serde_json::json!({}),
            CoreEvent::MetricsConfigChanged { pipeline_id } => serde_json::json!(pipeline_id),
            CoreEvent::MetricsFlushed { pipeline_id } => {
                let mut json = serde_json::json!({});
                if let Some(pid) = pipeline_id {
                    json["pipeline_id"] = serde_json::json!(pid);
                }
                json
            }
            CoreEvent::ProviderStatusUpdated { provider } => serde_json::json!({
                "provider": provider,
            }),
            CoreEvent::ProviderHealthAlert {
                alert_id,
                notification_key,
                provider_ids,
                level,
                title,
                body,
                link,
                recovered,
                ..
            } => serde_json::json!({
                "alert_id": alert_id,
                "notification_key": notification_key,
                "provider_ids": provider_ids,
                "level": level,
                "title": title,
                "body": body,
                "link": link,
                "recovered": recovered,
            }),
            CoreEvent::MigrationProgress {
                step,
                step_index,
                total_steps,
                message,
            } => serde_json::json!({
                "step": step,
                "step_index": step_index,
                "total_steps": total_steps,
                "message": message,
            }),
            CoreEvent::MigrationComplete { success, summary } => serde_json::json!({
                "success": success,
                "summary": summary,
            }),
            CoreEvent::ProviderAdded {
                provider,
                timestamp,
            } => serde_json::json!({
                "provider": provider,
                "action": "added",
                "timestamp": timestamp,
            }),
            CoreEvent::ProviderUpdated {
                provider,
                timestamp,
            } => serde_json::json!({
                "provider": provider,
                "action": "updated",
                "timestamp": timestamp,
            }),
            CoreEvent::ProviderRemoved {
                provider,
                timestamp,
            } => serde_json::json!({
                "provider": provider,
                "action": "removed",
                "timestamp": timestamp,
            }),
            CoreEvent::PipelineCacheInvalidated {
                provider_id,
                reason,
            } => {
                let mut json = serde_json::json!({
                    "reason": reason,
                });
                if let Some(pid) = provider_id {
                    json["providerId"] = serde_json::json!(pid);
                }
                json
            }
            CoreEvent::RunHistoryCacheInvalidated { pipeline_id } => {
                let mut json = serde_json::json!({});
                if let Some(pid) = pipeline_id {
                    json["pipeline_id"] = serde_json::json!(pid);
                }
                json
            }
            CoreEvent::VaultUnlocked => serde_json::json!({}),
            CoreEvent::VaultLocked => serde_json::json!({}),
            CoreEvent::RunLogProgress {
                pipeline_id,
                run_number,
                node_id,
                received_bytes,
            } => serde_json::json!({
                "pipelineId": pipeline_id,
                "runNumber": run_number,
                "nodeId": node_id,
                "receivedBytes": received_bytes,
            }),
            CoreEvent::RunLogReady {
                pipeline_id,
                run_number,
                node_id,
                revision,
            } => serde_json::json!({
                "pipelineId": pipeline_id,
                "runNumber": run_number,
                "nodeId": node_id,
                "revision": revision,
            }),
            CoreEvent::RunLogFailed {
                pipeline_id,
                run_number,
                node_id,
                message,
            } => serde_json::json!({
                "pipelineId": pipeline_id,
                "runNumber": run_number,
                "nodeId": node_id,
                "message": message,
            }),
        }
    }
}

#[async_trait]
pub trait EventBus: Send + Sync {
    async fn emit(&self, event: CoreEvent);

    async fn emit_to(&self, target: &str, event: CoreEvent);
}

pub struct NoOpEventBus;

#[async_trait]
impl EventBus for NoOpEventBus {
    async fn emit(&self, _event: CoreEvent) {}

    async fn emit_to(&self, _target: &str, _event: CoreEvent) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_names() {
        assert_eq!(
            CoreEvent::ProvidersChanged.event_name(),
            "providers-changed"
        );
        assert_eq!(
            CoreEvent::PipelineViewsChanged.event_name(),
            "pipeline-views-changed"
        );
        assert_eq!(
            CoreEvent::PipelineStatusChanged {
                pipeline_ids: vec!["a".to_string()]
            }
            .event_name(),
            "pipeline-status-changed"
        );
    }

    #[test]
    fn test_event_serialization() {
        let event = CoreEvent::PipelineStatusChanged {
            pipeline_ids: vec!["github__1__web".to_string(), "github__1__api".to_string()],
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("PipelineStatusChanged"));
        assert!(json.contains("github__1__web"));
        assert_eq!(
            event.to_json_payload(),
            serde_json::json!({ "pipeline_ids": ["github__1__web", "github__1__api"] })
        );
    }

    #[test]
    fn test_provider_health_alert_wire_format() {
        let alert = |link| CoreEvent::ProviderHealthAlert {
            alert_id: "alert-7".to_string(),
            alert_key: "incident:7".to_string(),
            notification_key: "status-page:feed".to_string(),
            provider_ids: vec![1, 2],
            level: HealthLevel::MajorOutage,
            title: "GitHub is down".to_string(),
            body: "Investigating".to_string(),
            link,
            recovered: false,
        };
        let with_link = alert(Some(AlertLink {
            url: "https://www.githubstatus.com/incidents/abc".to_string(),
            kind: AlertLinkKind::Incident,
        }));

        assert_eq!(with_link.event_name(), "provider-health-alert");
        assert_eq!(
            with_link.to_json_payload(),
            serde_json::json!({
                "alert_id": "alert-7",
                "notification_key": "status-page:feed",
                "provider_ids": [1, 2],
                "level": "major_outage",
                "title": "GitHub is down",
                "body": "Investigating",
                "link": {
                    "url": "https://www.githubstatus.com/incidents/abc",
                    "kind": "incident"
                },
                "recovered": false,
            })
        );
        assert_eq!(
            alert(None).to_json_payload()["link"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn test_vault_lock_and_pipelines_failed_wire_format() {
        assert_eq!(CoreEvent::VaultLocked.event_name(), "vault-locked");
        assert_eq!(
            CoreEvent::VaultLocked.to_json_payload(),
            serde_json::json!({})
        );

        let failed = CoreEvent::PipelinesFailed {
            pipelines: Vec::new(),
        };
        assert_eq!(failed.event_name(), "pipelines-failed");
        assert_eq!(
            failed.to_json_payload(),
            serde_json::json!({ "pipelines": [] })
        );
    }
}
