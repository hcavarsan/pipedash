use std::collections::HashMap;

use async_trait::async_trait;
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

impl ProviderFreshness {
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

    async fn fetch_pipelines_paginated(
        &self, page: usize, page_size: usize,
    ) -> DomainResult<pipedash_plugin_api::PaginatedResponse<Pipeline>> {
        use pipedash_plugin_api::PaginatedResponse;

        let all_pipelines = self.fetch_pipelines().await?;
        let total_count = all_pipelines.len();
        let start = (page - 1) * page_size;
        let end = start + page_size;

        let pipelines = if start < total_count {
            all_pipelines[start..end.min(total_count)].to_vec()
        } else {
            Vec::new()
        };

        Ok(PaginatedResponse::new(
            pipelines,
            page,
            page_size,
            total_count,
        ))
    }

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

    async fn trigger_pipeline(&self, params: TriggerParams) -> DomainResult<String>;

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()>;

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

    #[allow(dead_code)]
    async fn validate_credentials(&self) -> DomainResult<bool>;

    #[allow(dead_code)]
    fn provider_type(&self) -> &str;

    fn capabilities(&self) -> &pipedash_plugin_api::PluginCapabilities;

    fn renew_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        None
    }

    fn status_source(&self) -> Option<&pipedash_plugin_api::StatusSource>;

    async fn check_health(&self) -> DomainResult<Vec<pipedash_plugin_api::ComponentHealth>>;

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &PlainDialect
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
mod tests {
    use chrono::{
        DateTime,
        Duration,
        Utc,
    };

    use super::*;

    fn now() -> DateTime<Utc> {
        "2026-10-28T12:00:00Z".parse().unwrap()
    }

    fn compute(
        status: FetchStatus, level: HealthLevel, fetched_ago: Option<Duration>,
        paused_in: Option<Duration>, interval: i64,
    ) -> ProviderFreshness {
        ProviderFreshness::compute(
            &status,
            level,
            fetched_ago.map(|ago| now() - ago),
            paused_in.map(|ahead| now() + ahead),
            interval,
            now(),
        )
    }

    fn plain(state: FreshnessState) -> ProviderFreshness {
        ProviderFreshness {
            state,
            stale_at: None,
        }
    }

    #[test]
    fn error_status_is_down() {
        let f = compute(
            FetchStatus::Error,
            HealthLevel::Operational,
            Some(Duration::seconds(5)),
            None,
            30,
        );
        assert_eq!(f, plain(FreshnessState::Down));
    }

    #[test]
    fn partial_and_major_outage_are_down_even_when_never_fetched() {
        for level in [HealthLevel::PartialOutage, HealthLevel::MajorOutage] {
            let f = compute(FetchStatus::Never, level, None, None, 30);
            assert_eq!(f, plain(FreshnessState::Down));
        }
        let f = compute(
            FetchStatus::Success,
            HealthLevel::Degraded,
            Some(Duration::seconds(5)),
            None,
            30,
        );
        assert_eq!(f.state, FreshnessState::Fresh);
    }

    #[test]
    fn never_fetched_is_never() {
        let f = compute(FetchStatus::Never, HealthLevel::Operational, None, None, 30);
        assert_eq!(f, plain(FreshnessState::Never));
    }

    #[test]
    fn paused_in_the_future_is_stale_but_past_pause_is_ignored() {
        let paused = compute(
            FetchStatus::Success,
            HealthLevel::Operational,
            Some(Duration::seconds(5)),
            Some(Duration::seconds(30)),
            30,
        );
        assert_eq!(paused, plain(FreshnessState::Stale));

        let elapsed = compute(
            FetchStatus::Success,
            HealthLevel::Operational,
            Some(Duration::seconds(5)),
            Some(Duration::seconds(-30)),
            30,
        );
        assert_eq!(elapsed.state, FreshnessState::Fresh);
    }

    #[test]
    fn old_fetch_is_stale() {
        let f = compute(
            FetchStatus::Success,
            HealthLevel::Operational,
            Some(Duration::seconds(601)),
            None,
            30,
        );
        assert_eq!(f, plain(FreshnessState::Stale));
    }

    #[test]
    fn threshold_is_the_larger_of_three_intervals_and_ten_minutes() {
        let at_ten_minutes = compute(
            FetchStatus::Success,
            HealthLevel::Operational,
            Some(Duration::seconds(600)),
            None,
            30,
        );
        assert_eq!(at_ten_minutes.state, FreshnessState::Fresh);

        let long_interval_inside = compute(
            FetchStatus::Success,
            HealthLevel::Operational,
            Some(Duration::seconds(1500)),
            None,
            600,
        );
        assert_eq!(long_interval_inside.state, FreshnessState::Fresh);

        let long_interval_outside = compute(
            FetchStatus::Success,
            HealthLevel::Operational,
            Some(Duration::seconds(1801)),
            None,
            600,
        );
        assert_eq!(long_interval_outside, plain(FreshnessState::Stale));
    }

    #[test]
    fn fresh_reports_when_it_turns_stale() {
        let f = compute(
            FetchStatus::Success,
            HealthLevel::Operational,
            Some(Duration::seconds(100)),
            None,
            600,
        );
        assert_eq!(f.state, FreshnessState::Fresh);
        assert_eq!(
            f.stale_at,
            Some(now() - Duration::seconds(100) + Duration::seconds(1800))
        );

        let short = compute(
            FetchStatus::Success,
            HealthLevel::Operational,
            Some(Duration::seconds(100)),
            None,
            30,
        );
        assert_eq!(
            short.stale_at,
            Some(now() - Duration::seconds(100) + Duration::seconds(600))
        );
    }

    #[test]
    fn serializes_snake_case_and_defaults_to_never() {
        let json = serde_json::to_value(ProviderFreshness::default()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "state": "never", "stale_at": null })
        );
    }

    #[test]
    fn recovery_asks_signed_in_providers_to_sign_in_again() {
        let recovery = ProviderRecovery::for_failure;
        assert_eq!(
            recovery(Some(FailureKind::Auth), true),
            Some(ProviderRecovery::SignIn)
        );
        assert_eq!(
            recovery(Some(FailureKind::Auth), false),
            Some(ProviderRecovery::Settings)
        );
        assert_eq!(
            recovery(Some(FailureKind::Tls), true),
            Some(ProviderRecovery::Settings)
        );
        assert_eq!(recovery(Some(FailureKind::Network), true), None);
        assert_eq!(recovery(None, true), None);
    }
}
