use std::collections::HashMap;

use async_trait::async_trait;

use crate::actions::{
    PipelineActionState,
    RunActionState,
    TriggerResult,
};
use crate::error::PluginResult;
use crate::health::{
    ClientOptions,
    ComponentHealth,
    StatusSource,
};
use crate::logs::{
    LogCursor,
    LogDialect,
    LogNode,
    LogRead,
    LogSink,
    PlainDialect,
    RunRef,
};
use crate::metadata::PluginMetadata;
use crate::presentation::{
    PipelineFacts,
    RunDisplay,
    RunFacts,
};
use crate::sign_in::{
    SignInClient,
    SignInRefreshRejection,
};
use crate::support::retain_matching;
use crate::types::*;
use crate::utils::selected_items;

pub mod defaults;

#[async_trait]
pub trait Plugin: Send + Sync {
    fn metadata(&self) -> &PluginMetadata;

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()>;

    fn client_options(&self, _config: &HashMap<String, String>) -> ClientOptions {
        ClientOptions::default()
    }

    fn status_source(&self) -> Option<&StatusSource> {
        None
    }

    async fn check_health(&self) -> PluginResult<Vec<ComponentHealth>> {
        Ok(Vec::new())
    }

    async fn validate_credentials(&self) -> PluginResult<bool>;

    async fn fetch_available_pipelines(
        &self, params: Option<crate::types::PaginationParams>,
    ) -> PluginResult<crate::types::PaginatedAvailablePipelines> {
        let _ = params;
        Ok(crate::types::PaginatedResponse::empty())
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<crate::types::Organization>> {
        Ok(Vec::new())
    }

    async fn fetch_pipeline_catalog(&self) -> PluginResult<crate::types::PipelineCatalog> {
        Err(crate::error::PluginError::NotSupported(
            "Pipeline catalog not supported by this provider".to_string(),
        ))
    }

    async fn ids_with_ci(&self, ids: Vec<String>) -> PluginResult<Vec<String>> {
        Ok(ids)
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &PlainDialect
    }

    fn log_tree_version(&self) -> u32 {
        0
    }

    async fn fetch_log_nodes(&self, _run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        Err(crate::error::PluginError::NotSupported(
            "Run logs not supported by this provider".to_string(),
        ))
    }

    async fn read_log(
        &self, _run: RunRef<'_>, _node_id: &str, _from: Option<&LogCursor>, _sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        Err(crate::error::PluginError::NotSupported(
            "Run logs not supported by this provider".to_string(),
        ))
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>,
        params: Option<crate::types::PaginationParams>,
    ) -> PluginResult<crate::types::PaginatedAvailablePipelines> {
        let response = self.fetch_available_pipelines(params.clone()).await?;

        let mut filtered_items = response.items;

        if let Some(org_filter) = org {
            filtered_items.retain(|p| p.organization.as_ref() == Some(&org_filter));
        }

        if let Some(search_term) = search {
            retain_matching(&mut filtered_items, &search_term);
        }

        let total_count = filtered_items.len();
        let params = params.unwrap_or_default();

        Ok(crate::types::PaginatedResponse::new(
            filtered_items,
            params.page,
            params.page_size,
            total_count,
        ))
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>>;

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>>;

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun>;

    async fn fetch_recent_runs(
        &self, _pipeline_ids: &[String], _limit: usize,
    ) -> PluginResult<HashMap<String, Vec<PipelineRun>>> {
        Err(crate::error::PluginError::NotSupported(
            "Batched run history not supported by this provider".to_string(),
        ))
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<TriggerResult>;

    async fn cancel_run(&self, _pipeline_id: &str, _run_number: i64) -> PluginResult<()> {
        Err(crate::error::PluginError::NotSupported(
            "Run cancellation not supported by this provider".to_string(),
        ))
    }

    async fn force_cancel_run(&self, _pipeline_id: &str, _run_number: i64) -> PluginResult<()> {
        Err(crate::error::PluginError::NotSupported(
            "Force cancellation not supported by this provider".to_string(),
        ))
    }

    async fn review_run(
        &self, _pipeline_id: &str, _run_number: i64, _decision: ReviewDecision,
    ) -> PluginResult<()> {
        Err(crate::error::PluginError::NotSupported(
            "Run review not supported by this provider".to_string(),
        ))
    }

    async fn rerun_failed_jobs(&self, _pipeline_id: &str, _run_number: i64) -> PluginResult<()> {
        Err(crate::error::PluginError::NotSupported(
            "Re-running failed jobs not supported by this provider".to_string(),
        ))
    }

    async fn rerun_run(&self, _pipeline_id: &str, _run_number: i64) -> PluginResult<()> {
        Err(crate::error::PluginError::NotSupported(
            "Re-running runs not supported by this provider".to_string(),
        ))
    }

    async fn enable_pipeline(&self, _pipeline_id: &str) -> PluginResult<()> {
        Err(crate::error::PluginError::NotSupported(
            "Enabling pipelines not supported by this provider".to_string(),
        ))
    }

    async fn fetch_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        Ok(Vec::new())
    }

    async fn fetch_rate_limit(&self) -> PluginResult<Option<RateLimit>> {
        Ok(None)
    }

    async fn get_field_options(
        &self, _field_key: &str, _config: &HashMap<String, String>,
    ) -> PluginResult<Vec<String>> {
        Ok(Vec::new())
    }

    async fn sign_in_client(
        &self, _config: &HashMap<String, String>, _http_client: &reqwest::Client,
    ) -> PluginResult<SignInClient> {
        Err(crate::error::PluginError::NotSupported(
            "Sign-in is not supported by this provider".to_string(),
        ))
    }

    fn sign_in_refresh_rejections(&self) -> &'static [SignInRefreshRejection] {
        &[]
    }

    async fn check_permissions(&self) -> PluginResult<PermissionStatus> {
        let required_permissions = &self.metadata().required_permissions;
        let permissions = required_permissions
            .iter()
            .map(|p| PermissionCheck {
                permission: p.clone(),
                granted: true,
            })
            .collect();

        Ok(PermissionStatus {
            permissions,
            all_granted: true,
            checked_at: chrono::Utc::now(),
            metadata: std::collections::HashMap::new(),
            credential_label: None,
        })
    }

    fn get_feature_availability(&self, status: &PermissionStatus) -> Vec<FeatureAvailability> {
        let features = &self.metadata().features;
        let granted_perms: std::collections::HashSet<String> = status
            .permissions
            .iter()
            .filter(|p| p.granted)
            .map(|p| p.permission.name.clone())
            .collect();

        features
            .iter()
            .map(|feature| {
                let missing: Vec<String> = feature
                    .required_permissions
                    .iter()
                    .filter(|p| !granted_perms.contains(*p))
                    .cloned()
                    .collect();

                FeatureAvailability {
                    feature: feature.clone(),
                    available: missing.is_empty(),
                    missing_permissions: missing,
                }
            })
            .collect()
    }

    fn can_archive_run(
        &self, _pipeline_metadata: &HashMap<String, serde_json::Value>,
        _run_metadata: &HashMap<String, serde_json::Value>, _status: PipelineStatus,
    ) -> bool {
        false
    }

    fn selection_required(&self, _config: &HashMap<String, String>) -> bool {
        true
    }

    fn configured_repositories(&self, config: &HashMap<String, String>) -> Vec<String> {
        selected_items(config).map(str::to_string).collect()
    }

    fn pipeline_actions(&self, pipeline: &PipelineFacts<'_>) -> PipelineActionState {
        defaults::pipeline_actions(&self.metadata().capabilities, pipeline)
    }

    fn run_actions(&self, _pipeline: &PipelineFacts<'_>, run: &RunFacts<'_>) -> RunActionState {
        defaults::run_actions(&self.metadata().capabilities, run)
    }

    fn run_display(&self, _pipeline: &PipelineFacts<'_>, run: &RunFacts<'_>) -> RunDisplay {
        defaults::run_display(run)
    }

    fn trigger_values(
        &self, _pipeline: &PipelineFacts<'_>, previous: Option<&RunFacts<'_>>,
        parameters: &[WorkflowParameter],
    ) -> serde_json::Map<String, serde_json::Value> {
        defaults::trigger_values(previous, parameters)
    }
}

#[cfg(test)]
mod tests;
