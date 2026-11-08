use std::collections::HashMap;

use async_trait::async_trait;
use pipedash_plugin_api::TriggerResult;
use serde::{
    Deserialize,
    Serialize,
};

use super::credential::SignInCredential;
use super::error::{
    DomainError,
    DomainResult,
    FailureKind,
};
use super::health::{
    HealthLevel,
    ProviderHealth,
};
use super::pipeline::{
    Pipeline,
    PipelineRun,
    TriggerParams,
};
use super::provider_attention::ProviderAttention;
use super::run_logs::{
    LogCursor,
    LogDialect,
    LogNode,
    LogRead,
    LogSink,
    PlainDialect,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: Option<i64>,
    pub name: String,
    pub provider_type: String,
    pub token: String,
    pub config: HashMap<String, String>,
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval: i64,
    #[serde(default)]
    pub version: Option<i64>,
}

fn default_refresh_interval() -> i64 {
    30
}

impl ProviderConfig {
    pub fn display_name(&self) -> &str {
        self.config
            .get("display_name")
            .map(|s| s.as_str())
            .unwrap_or(&self.name)
    }

    pub fn plugin_config(&self) -> HashMap<String, String> {
        let mut config = self.config.clone();
        config.insert("token".to_string(), SignInCredential::bearer(&self.token));
        config
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FetchStatus {
    Success,
    Error,
    Never,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderImportResult {
    pub added: Vec<String>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSummary {
    pub id: i64,
    pub name: String,
    pub provider_type: String,
    pub icon: Option<String>,
    pub pipeline_count: usize,
    pub last_updated: Option<chrono::DateTime<chrono::Utc>>,
    pub refresh_interval: i64,
    pub configured_repositories: Vec<String>,
    pub last_fetch_status: FetchStatus,
    pub last_fetch_error: Option<String>,
    pub last_fetch_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub last_success_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub paused_until: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub last_fetch_error_kind: Option<FailureKind>,
    #[serde(default)]
    pub health: ProviderHealth,
    pub version: i64,
    #[serde(default)]
    pub freshness: ProviderFreshness,
    #[serde(default)]
    pub recovery: Option<ProviderRecovery>,
    #[serde(default)]
    pub attention: ProviderAttention,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderRecovery {
    SignIn,
    Settings,
}

impl ProviderRecovery {
    pub fn for_failure(kind: Option<FailureKind>, signed_in: bool) -> Option<Self> {
        match kind? {
            FailureKind::Auth if signed_in => Some(Self::SignIn),
            FailureKind::Auth
            | FailureKind::Permission
            | FailureKind::Config
            | FailureKind::Tls => Some(Self::Settings),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FreshnessState {
    Fresh,
    Stale,
    Down,
    #[default]
    Never,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderFreshness {
    pub state: FreshnessState,
    pub stale_at: Option<chrono::DateTime<chrono::Utc>>,
}

const MIN_STALE_SECS: i64 = 10 * 60;

impl FreshnessState {
    fn severity(self) -> u8 {
        match self {
            Self::Fresh => 0,
            Self::Never => 1,
            Self::Stale => 2,
            Self::Down => 3,
        }
    }
}

impl ProviderFreshness {
    pub fn aggregate<'a>(items: impl IntoIterator<Item = &'a ProviderFreshness>) -> Self {
        let mut state = FreshnessState::Fresh;
        let mut stale_at = None;
        for item in items {
            if item.state.severity() > state.severity() {
                state = item.state;
            }
            if item.state == FreshnessState::Fresh {
                stale_at = [stale_at, item.stale_at].into_iter().flatten().min();
            }
        }
        Self {
            state,
            stale_at: stale_at.filter(|_| state == FreshnessState::Fresh),
        }
    }

    pub fn compute(
        status: &FetchStatus, level: HealthLevel,
        last_fetch_at: Option<chrono::DateTime<chrono::Utc>>,
        paused_until: Option<chrono::DateTime<chrono::Utc>>, refresh_interval: i64,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Self {
        let state = |state| Self {
            state,
            stale_at: None,
        };
        if matches!(status, FetchStatus::Error)
            || matches!(level, HealthLevel::PartialOutage | HealthLevel::MajorOutage)
        {
            return state(FreshnessState::Down);
        }
        if matches!(status, FetchStatus::Never) {
            return state(FreshnessState::Never);
        }
        let window =
            chrono::TimeDelta::try_seconds(refresh_interval.saturating_mul(3).max(MIN_STALE_SECS))
                .unwrap_or(chrono::TimeDelta::MAX);
        let stale_at = last_fetch_at.and_then(|at| at.checked_add_signed(window));
        if paused_until.is_some_and(|until| until > now) || stale_at.is_some_and(|at| now > at) {
            return state(FreshnessState::Stale);
        }
        Self {
            state: FreshnessState::Fresh,
            stale_at,
        }
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    async fn fetch_pipelines(&self) -> DomainResult<Vec<Pipeline>>;

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> DomainResult<Vec<PipelineRun>>;

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<PipelineRun>;

    async fn fetch_recent_runs(
        &self, _pipeline_ids: &[String], _limit: usize,
    ) -> DomainResult<HashMap<String, Vec<PipelineRun>>> {
        Err(DomainError::NotSupported(
            "Batched run history is not supported by this provider".into(),
        ))
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> DomainResult<TriggerResult>;

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()>;

    async fn force_cancel_run(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()>;

    async fn review_run(
        &self, pipeline_id: &str, run_number: i64, decision: pipedash_plugin_api::ReviewDecision,
    ) -> DomainResult<()>;

    async fn rerun_failed_jobs(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()>;

    async fn rerun_run(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()>;

    async fn enable_pipeline(&self, pipeline_id: &str) -> DomainResult<()>;

    async fn get_workflow_parameters(
        &self, workflow_id: &str,
    ) -> DomainResult<Vec<pipedash_plugin_api::WorkflowParameter>>;

    async fn fetch_rate_limit(&self) -> DomainResult<Option<pipedash_plugin_api::RateLimit>>;

    fn capabilities(&self) -> &pipedash_plugin_api::PluginCapabilities;

    fn renew_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        None
    }

    fn run_history_policy(&self) -> pipedash_plugin_api::RunHistoryPolicy {
        pipedash_plugin_api::RunHistoryPolicy::default()
    }

    fn can_archive_run(&self, _pipeline: &Pipeline, _run: &PipelineRun) -> bool {
        false
    }

    fn status_source(&self) -> Option<&pipedash_plugin_api::StatusSource>;

    async fn check_health(&self) -> DomainResult<Vec<pipedash_plugin_api::ComponentHealth>>;

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &PlainDialect
    }

    fn log_tree_version(&self) -> u32 {
        0
    }

    async fn fetch_log_nodes(&self, _run: &PipelineRun) -> DomainResult<Vec<LogNode>> {
        Err(DomainError::NotSupported(
            "Run logs are not supported by this provider".into(),
        ))
    }

    async fn read_log(
        &self, _run: &PipelineRun, _node_id: &str, _from: Option<&LogCursor>, _sink: &mut LogSink,
    ) -> DomainResult<LogRead> {
        Err(DomainError::NotSupported(
            "Run logs are not supported by this provider".into(),
        ))
    }
}

#[cfg(test)]
mod tests;
