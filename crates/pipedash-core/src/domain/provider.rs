use std::collections::HashMap;

use async_trait::async_trait;
use serde::{
    Deserialize,
    Serialize,
};

use super::error::{
    DomainResult,
    FailureKind,
};
use super::health::ProviderHealth;
use super::pipeline::{
    Pipeline,
    PipelineRun,
    TriggerParams,
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
    pub paused_until: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub last_fetch_error_kind: Option<FailureKind>,
    #[serde(default)]
    pub health: ProviderHealth,
    pub version: i64,
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

    async fn trigger_pipeline(&self, params: TriggerParams) -> DomainResult<String>;

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()>;

    async fn review_run(
        &self, pipeline_id: &str, run_number: i64, decision: pipedash_plugin_api::ReviewDecision,
    ) -> DomainResult<()>;

    async fn get_workflow_parameters(
        &self, workflow_id: &str,
    ) -> DomainResult<Vec<pipedash_plugin_api::WorkflowParameter>>;

    async fn fetch_rate_limit(&self) -> DomainResult<Option<pipedash_plugin_api::RateLimit>>;

    #[allow(dead_code)]
    async fn validate_credentials(&self) -> DomainResult<bool>;

    #[allow(dead_code)]
    fn provider_type(&self) -> &str;

    fn capabilities(&self) -> &pipedash_plugin_api::PluginCapabilities;

    fn status_source(&self) -> Option<&pipedash_plugin_api::StatusSource>;

    async fn check_health(&self) -> DomainResult<Vec<pipedash_plugin_api::ComponentHealth>>;
}
