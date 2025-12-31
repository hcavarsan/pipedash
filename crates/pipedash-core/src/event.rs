use async_trait::async_trait;
use serde::{
    Deserialize,
    Serialize,
};

use crate::domain::{
    provider::ProviderSummary,
    Pipeline,
};

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

    PipelinesFetched {
        provider_id: i64,
    },

    PipelinesFetchError {
        error: String,
    },

    PipelinesUpdated {
        pipelines: Vec<Pipeline>,
        provider_id: Option<i64>,
        timestamp: i64,
    },

    PipelineStatusChanged {
        pipelines: Vec<Pipeline>,
    },

    RunTriggered {
        workflow_id: String,
    },

    RunCancelled {
        pipeline_id: String,
    },

    RefreshError {
        error: String,
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
        provider_id: i64,
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
}

impl CoreEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            CoreEvent::ProvidersChanged => "providers-changed",
            CoreEvent::PipelinesFetched { .. } => "pipelines-fetched",
            CoreEvent::PipelinesFetchError { .. } => "pipelines-fetch-error",
            CoreEvent::PipelinesUpdated { .. } => "pipelines-updated",
            CoreEvent::PipelineStatusChanged { .. } => "pipeline-status-changed",
            CoreEvent::RunTriggered { .. } => "run-triggered",
            CoreEvent::RunCancelled { .. } => "run-cancelled",
            CoreEvent::RefreshError { .. } => "refresh-error",
            CoreEvent::MetricsGenerated { .. } => "metrics-generated",
            CoreEvent::MetricsGlobalConfigChanged => "metrics-global-config-changed",
            CoreEvent::MetricsConfigChanged { .. } => "metrics-config-changed",
            CoreEvent::MetricsFlushed { .. } => "metrics-flushed",
            CoreEvent::ProviderStatusUpdated { .. } => "provider-status-updated",
            CoreEvent::MigrationProgress { .. } => "migration-progress",
            CoreEvent::MigrationComplete { .. } => "migration-complete",
            CoreEvent::ProviderAdded { .. } => "provider-added",
            CoreEvent::ProviderUpdated { .. } => "provider-updated",
            CoreEvent::ProviderRemoved { .. } => "provider-removed",
            CoreEvent::PipelineCacheInvalidated { .. } => "pipeline-cache-invalidated",
            CoreEvent::RunHistoryCacheInvalidated { .. } => "run-history-cache-invalidated",
            CoreEvent::VaultUnlocked => "vault-unlocked",
        }
    }

    pub fn to_json_payload(&self) -> serde_json::Value {
        match self {
            CoreEvent::ProvidersChanged => serde_json::json!({}),
            CoreEvent::PipelinesFetched { provider_id } => serde_json::json!(provider_id),
            CoreEvent::PipelinesFetchError { error } => serde_json::json!(error),
            CoreEvent::PipelinesUpdated {
                pipelines,
                provider_id,
                timestamp,
            } => {
                let mut json = serde_json::json!({
                    "pipelines": pipelines,
                    "timestamp": timestamp,
                });
                if let Some(pid) = provider_id {
                    json["providerId"] = serde_json::json!(pid);
                }
                json
            }
            CoreEvent::PipelineStatusChanged { pipelines } => {
                serde_json::to_value(pipelines).unwrap_or_default()
            }
            CoreEvent::RunTriggered { workflow_id } => serde_json::json!(workflow_id),
            CoreEvent::RunCancelled { pipeline_id } => serde_json::json!(pipeline_id),
            CoreEvent::RefreshError { error } => serde_json::json!(error),
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
            CoreEvent::ProviderStatusUpdated { provider_id } => serde_json::json!(provider_id),
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
            CoreEvent::PipelinesFetched { provider_id: 1 }.event_name(),
            "pipelines-fetched"
        );
    }

    #[test]
    fn test_event_serialization() {
        let event = CoreEvent::PipelinesFetched { provider_id: 42 };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("PipelinesFetched"));
        assert!(json.contains("42"));
    }
}
