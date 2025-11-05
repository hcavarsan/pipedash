use std::sync::Arc;

use async_trait::async_trait;
use pipedash_plugin_api::{
    Plugin as PluginTrait,
    PluginError,
};
use tokio::sync::Mutex;

use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
    PipelineRun,
    Provider,
    TriggerParams,
};

pub struct PluginAdapter {
    plugin: Arc<Mutex<Box<dyn PluginTrait>>>,
    provider_type: String,
}

impl PluginAdapter {
    pub fn new(plugin: Box<dyn PluginTrait>) -> Self {
        let provider_type = plugin.provider_type().to_string();
        Self {
            plugin: Arc::new(Mutex::new(plugin)),
            provider_type,
        }
    }
}

#[async_trait]
impl Provider for PluginAdapter {
    async fn fetch_pipelines(&self) -> DomainResult<Vec<Pipeline>> {
        let plugin = self.plugin.lock().await;
        let plugin_pipelines = plugin
            .fetch_pipelines()
            .await
            .map_err(convert_plugin_error)?;

        Ok(plugin_pipelines.into_iter().map(convert_pipeline).collect())
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> DomainResult<Vec<PipelineRun>> {
        let plugin = self.plugin.lock().await;
        let plugin_runs = plugin
            .fetch_run_history(pipeline_id, limit)
            .await
            .map_err(convert_plugin_error)?;

        Ok(plugin_runs.into_iter().map(convert_pipeline_run).collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<PipelineRun> {
        let plugin = self.plugin.lock().await;
        let plugin_run = plugin
            .fetch_run_details(pipeline_id, run_number)
            .await
            .map_err(convert_plugin_error)?;

        Ok(convert_pipeline_run(plugin_run))
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> DomainResult<String> {
        let plugin = self.plugin.lock().await;

        let plugin_params = pipedash_plugin_api::TriggerParams {
            workflow_id: params.workflow_id,
            inputs: params.inputs,
        };
        plugin
            .trigger_pipeline(plugin_params)
            .await
            .map_err(convert_plugin_error)
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()> {
        let plugin = self.plugin.lock().await;
        plugin
            .cancel_run(pipeline_id, run_number)
            .await
            .map_err(convert_plugin_error)
    }

    async fn get_workflow_parameters(
        &self, workflow_id: &str,
    ) -> DomainResult<Vec<pipedash_plugin_api::WorkflowParameter>> {
        let plugin = self.plugin.lock().await;
        plugin
            .fetch_workflow_parameters(workflow_id)
            .await
            .map_err(convert_plugin_error)
    }

    async fn validate_credentials(&self) -> DomainResult<bool> {
        let plugin = self.plugin.lock().await;
        plugin
            .validate_credentials()
            .await
            .map_err(convert_plugin_error)
    }

    fn provider_type(&self) -> &str {
        &self.provider_type
    }
}

fn convert_plugin_error(error: PluginError) -> DomainError {
    match error {
        PluginError::AuthenticationFailed(msg) => DomainError::AuthenticationFailed(msg),
        PluginError::ApiError(msg) => DomainError::ApiError(msg),
        PluginError::InvalidConfig(msg) => DomainError::InvalidConfig(msg),
        PluginError::NotSupported(msg) => DomainError::NotSupported(msg),
        PluginError::Internal(msg) => DomainError::InternalError(msg),
        PluginError::NetworkError(msg) => DomainError::NetworkError(msg),
        PluginError::PipelineNotFound(msg) => DomainError::PipelineNotFound(msg),
        PluginError::ProviderNotSupported(msg) => DomainError::ProviderError(msg),
        PluginError::SerializationError(msg) => DomainError::InternalError(msg),
        PluginError::DatabaseError(msg) => DomainError::DatabaseError(msg),
    }
}

fn convert_pipeline(plugin_pipeline: pipedash_plugin_api::Pipeline) -> Pipeline {
    Pipeline {
        id: plugin_pipeline.id,
        provider_id: plugin_pipeline.provider_id,
        provider_type: plugin_pipeline.provider_type,
        name: plugin_pipeline.name,
        status: convert_status(plugin_pipeline.status),
        last_run: plugin_pipeline.last_run,
        last_updated: plugin_pipeline.last_updated,
        repository: plugin_pipeline.repository,
        branch: plugin_pipeline.branch,
        workflow_file: plugin_pipeline.workflow_file,
        metadata: plugin_pipeline.metadata,
    }
}

fn convert_pipeline_run(plugin_run: pipedash_plugin_api::PipelineRun) -> PipelineRun {
    PipelineRun {
        id: plugin_run.id,
        pipeline_id: plugin_run.pipeline_id,
        run_number: plugin_run.run_number,
        status: convert_status(plugin_run.status),
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

fn convert_status(
    plugin_status: pipedash_plugin_api::PipelineStatus,
) -> crate::domain::pipeline::PipelineStatus {
    use pipedash_plugin_api::PipelineStatus as PluginStatus;

    use crate::domain::pipeline::PipelineStatus as DomainStatus;

    match plugin_status {
        PluginStatus::Success => DomainStatus::Success,
        PluginStatus::Failed => DomainStatus::Failed,
        PluginStatus::Running => DomainStatus::Running,
        PluginStatus::Pending => DomainStatus::Pending,
        PluginStatus::Cancelled => DomainStatus::Cancelled,
        PluginStatus::Skipped => DomainStatus::Skipped,
    }
}
