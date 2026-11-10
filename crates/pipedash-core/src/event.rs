use async_trait::async_trait;
use serde::{
    Deserialize,
    Serialize,
};

use crate::application::ObservedNotification;
use crate::domain::{
    provider::ProviderSummary,
    HealthLevel,
    Pipeline,
    PipelineRun,
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

    PipelinesFinished {
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

    RunHistoryCacheInvalidated {
        pipeline_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        run_number: Option<i64>,
    },

    VaultUnlocked,

    VaultLocked,

    DevicePaired {
        id: String,
        name: String,
    },

    DeviceRevoked {
        id: String,
    },

    AlertRecorded {
        id: String,
        seq: i64,
    },

    NotificationObserved {
        notification: ObservedNotification,
    },

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

    RunLogTreeChanged {
        pipeline_id: String,
        run_number: i64,
    },

    RunUpdated {
        pipeline_id: String,
        run_number: i64,
        run: Box<PipelineRun>,
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
            CoreEvent::PipelinesFinished { .. } => "pipelines-finished",
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
            CoreEvent::RunHistoryCacheInvalidated { .. } => "run-history-cache-invalidated",
            CoreEvent::VaultUnlocked => "vault-unlocked",
            CoreEvent::VaultLocked => "vault-locked",
            CoreEvent::DevicePaired { .. } => "device-paired",
            CoreEvent::DeviceRevoked { .. } => "device-revoked",
            CoreEvent::AlertRecorded { .. } => "alert-recorded",
            CoreEvent::NotificationObserved { .. } => "notification-observed",
            CoreEvent::RunLogProgress { .. } => "run-log-progress",
            CoreEvent::RunLogReady { .. } => "run-log-ready",
            CoreEvent::RunLogFailed { .. } => "run-log-failed",
            CoreEvent::RunLogTreeChanged { .. } => "run-log-tree-changed",
            CoreEvent::RunUpdated { .. } => "run-updated",
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

mod payload;

#[cfg(test)]
mod tests;
