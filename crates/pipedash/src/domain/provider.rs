use std::collections::HashMap;

use async_trait::async_trait;
use serde::{
    Deserialize,
    Serialize,
};

use super::error::DomainResult;
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
}

fn default_refresh_interval() -> i64 {
    30
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

    async fn trigger_pipeline(&self, params: TriggerParams) -> DomainResult<String>;

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()>;

    async fn get_workflow_parameters(
        &self, workflow_id: &str,
    ) -> DomainResult<Vec<pipedash_plugin_api::WorkflowParameter>>;

    async fn validate_credentials(&self) -> DomainResult<bool>;

    fn provider_type(&self) -> &str;
}
