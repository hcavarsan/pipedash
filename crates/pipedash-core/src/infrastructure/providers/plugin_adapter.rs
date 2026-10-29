use std::sync::Arc;

use async_trait::async_trait;
use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    Plugin as PluginTrait,
    RunRef,
};

use crate::domain::{
    DomainError,
    DomainResult,
    LogCursor,
    LogDialect,
    LogNode,
    LogRead,
    LogSink,
    Pipeline,
    PipelineRun,
    Provider,
    TriggerParams,
};

pub struct PluginAdapter {
    plugin: Arc<dyn PluginTrait>,
    provider_type: String,
    provider_id: i64,
    renew_at: Option<DateTime<Utc>>,
}

impl PluginAdapter {
    pub fn new(
        plugin: Box<dyn PluginTrait>, provider_type: String, provider_id: i64,
        renew_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            plugin: Arc::from(plugin),
            provider_type,
            provider_id,
            renew_at,
        }
    }

    pub(crate) fn map_error(e: pipedash_plugin_api::PluginError) -> DomainError {
        match e {
            pipedash_plugin_api::PluginError::AuthenticationFailed(msg) => {
                DomainError::ProviderAuthenticationFailed(msg)
            }
            pipedash_plugin_api::PluginError::PermissionDenied(msg) => {
                DomainError::PermissionDenied(msg)
            }
            pipedash_plugin_api::PluginError::ApiError(msg) => DomainError::ApiError(msg),
            pipedash_plugin_api::PluginError::InvalidConfig(msg) => DomainError::InvalidConfig(msg),
            pipedash_plugin_api::PluginError::PipelineNotFound(msg) => {
                DomainError::PipelineNotFound(msg)
            }
            pipedash_plugin_api::PluginError::ProviderNotSupported(msg) => {
                DomainError::InvalidProviderType(msg)
            }
            pipedash_plugin_api::PluginError::NetworkError(msg) => DomainError::NetworkError(msg),
            pipedash_plugin_api::PluginError::SerializationError(msg) => {
                DomainError::ApiError(format!("Serialization error: {}", msg))
            }
            pipedash_plugin_api::PluginError::DatabaseError(msg) => DomainError::DatabaseError(msg),
            pipedash_plugin_api::PluginError::NotSupported(msg) => DomainError::NotSupported(msg),
            pipedash_plugin_api::PluginError::Internal(msg) => DomainError::InternalError(msg),
            pipedash_plugin_api::PluginError::RateLimited {
                message,
                retry_after,
            } => DomainError::RateLimited {
                message,
                retry_after,
            },
            pipedash_plugin_api::PluginError::ServiceUnavailable {
                message,
                retry_after,
            } => DomainError::ServiceUnavailable {
                message,
                retry_after,
            },
            pipedash_plugin_api::PluginError::Tls(msg) => DomainError::Tls(msg),
            e => DomainError::ApiError(e.to_string()),
        }
    }

    fn convert_pipeline(
        plugin_pipeline: pipedash_plugin_api::Pipeline, provider_id: i64, provider_type: &str,
    ) -> Pipeline {
        Pipeline {
            id: plugin_pipeline.id,
            provider_id,
            provider_type: provider_type.to_string(),
            name: plugin_pipeline.name,
            status: Self::convert_status(plugin_pipeline.status),
            last_run: plugin_pipeline.last_run,
            last_updated: plugin_pipeline.last_updated,
            repository: plugin_pipeline.repository,
            branch: plugin_pipeline.branch,
            workflow_file: plugin_pipeline.workflow_file,
            resource_state: plugin_pipeline.resource_state,
            metadata: plugin_pipeline.metadata,
            overdue_run: None,
        }
    }

    fn convert_status(
        status: pipedash_plugin_api::PipelineStatus,
    ) -> crate::domain::PipelineStatus {
        match status {
            pipedash_plugin_api::PipelineStatus::Success => crate::domain::PipelineStatus::Success,
            pipedash_plugin_api::PipelineStatus::Failed => crate::domain::PipelineStatus::Failed,
            pipedash_plugin_api::PipelineStatus::Degraded => {
                crate::domain::PipelineStatus::Degraded
            }
            pipedash_plugin_api::PipelineStatus::Running => crate::domain::PipelineStatus::Running,
            pipedash_plugin_api::PipelineStatus::Pending => crate::domain::PipelineStatus::Pending,
            pipedash_plugin_api::PipelineStatus::Cancelled => {
                crate::domain::PipelineStatus::Cancelled
            }
            pipedash_plugin_api::PipelineStatus::Skipped => crate::domain::PipelineStatus::Skipped,
        }
    }

    fn convert_run(plugin_run: pipedash_plugin_api::PipelineRun) -> PipelineRun {
        PipelineRun {
            id: plugin_run.id,
            pipeline_id: plugin_run.pipeline_id,
            run_number: plugin_run.run_number,
            status: Self::convert_status(plugin_run.status),
            started_at: plugin_run.started_at,
            concluded_at: plugin_run.concluded_at,
            duration_seconds: plugin_run.duration_seconds,
            logs_url: plugin_run.logs_url,
            commit_sha: plugin_run.commit_sha,
            commit_message: plugin_run.commit_message,
            branch: plugin_run.branch,
            actor: plugin_run.actor,
            inputs: plugin_run.inputs,
            metadata: plugin_run.metadata,
        }
    }

    fn run_ref(run: &PipelineRun) -> RunRef<'_> {
        RunRef {
            pipeline_id: &run.pipeline_id,
            run_number: run.run_number,
            run_id: &run.id,
            metadata: &run.metadata,
        }
    }
}

#[async_trait]
impl Provider for PluginAdapter {
    async fn fetch_pipelines(&self) -> DomainResult<Vec<Pipeline>> {
        let pipelines = self
            .plugin
            .fetch_pipelines()
            .await
            .map_err(Self::map_error)?;

        Ok(pipelines
            .into_iter()
            .map(|p| Self::convert_pipeline(p, self.provider_id, &self.provider_type))
            .collect())
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> DomainResult<Vec<PipelineRun>> {
        let runs = self
            .plugin
            .fetch_run_history(pipeline_id, limit)
            .await
            .map_err(Self::map_error)?;

        Ok(runs.into_iter().map(Self::convert_run).collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<PipelineRun> {
        let run = self
            .plugin
            .fetch_run_details(pipeline_id, run_number)
            .await
            .map_err(Self::map_error)?;

        Ok(Self::convert_run(run))
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> DomainResult<String> {
        let trigger_params = pipedash_plugin_api::TriggerParams {
            workflow_id: params.workflow_id,
            inputs: params.inputs,
        };

        self.plugin
            .trigger_pipeline(trigger_params)
            .await
            .map_err(Self::map_error)
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()> {
        self.plugin
            .cancel_run(pipeline_id, run_number)
            .await
            .map_err(Self::map_error)
    }

    async fn review_run(
        &self, pipeline_id: &str, run_number: i64, decision: pipedash_plugin_api::ReviewDecision,
    ) -> DomainResult<()> {
        self.plugin
            .review_run(pipeline_id, run_number, decision)
            .await
            .map_err(Self::map_error)
    }

    async fn rerun_failed_jobs(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()> {
        self.plugin
            .rerun_failed_jobs(pipeline_id, run_number)
            .await
            .map_err(Self::map_error)
    }

    async fn rerun_run(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()> {
        self.plugin
            .rerun_run(pipeline_id, run_number)
            .await
            .map_err(Self::map_error)
    }

    async fn get_workflow_parameters(
        &self, workflow_id: &str,
    ) -> DomainResult<Vec<pipedash_plugin_api::WorkflowParameter>> {
        self.plugin
            .fetch_workflow_parameters(workflow_id)
            .await
            .map_err(Self::map_error)
    }

    async fn fetch_rate_limit(&self) -> DomainResult<Option<pipedash_plugin_api::RateLimit>> {
        self.plugin
            .fetch_rate_limit()
            .await
            .map_err(Self::map_error)
    }

    async fn validate_credentials(&self) -> DomainResult<bool> {
        self.plugin
            .validate_credentials()
            .await
            .map_err(Self::map_error)
    }

    fn provider_type(&self) -> &str {
        &self.provider_type
    }

    fn capabilities(&self) -> &pipedash_plugin_api::PluginCapabilities {
        &self.plugin.metadata().capabilities
    }

    fn renew_at(&self) -> Option<DateTime<Utc>> {
        self.renew_at
    }

    fn status_source(&self) -> Option<&pipedash_plugin_api::StatusSource> {
        self.plugin.status_source()
    }

    async fn check_health(&self) -> DomainResult<Vec<pipedash_plugin_api::ComponentHealth>> {
        self.plugin.check_health().await.map_err(Self::map_error)
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        self.plugin.log_dialect()
    }

    async fn fetch_log_nodes(&self, run: &PipelineRun) -> DomainResult<Vec<LogNode>> {
        let nodes = self
            .plugin
            .fetch_log_nodes(Self::run_ref(run))
            .await
            .map_err(Self::map_error)?;
        Ok(nodes
            .into_iter()
            .map(|n| LogNode {
                id: n.id,
                parent_id: n.parent_id,
                kind: n.kind,
                name: n.name,
                status: Self::convert_status(n.status),
                started_at: n.started_at,
                finished_at: n.finished_at,
                log: n.log,
                web_url: n.web_url,
            })
            .collect())
    }

    async fn read_log(
        &self, run: &PipelineRun, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> DomainResult<LogRead> {
        self.plugin
            .read_log(Self::run_ref(run), node_id, from, sink)
            .await
            .map_err(Self::map_error)
    }
}
