use std::collections::HashMap;

use pipedash_plugin_api::kubernetes::KubeconfigSource;
use pipedash_plugin_api::pagination::paginate;
use pipedash_plugin_api::support::retain_matching;
use pipedash_plugin_api::*;
use tokio::sync::OnceCell;

use crate::client::ArgoClient;
use crate::types::Workflow;
use crate::{
    config,
    mapper,
    metadata,
};

mod catalog;

plugin_struct!(
    custom ArgoWorkflowsPlugin,
    metadata::create_metadata();
    client: OnceCell<ArgoClient> = OnceCell::new(),
    status_source: Option<StatusSource> = None,
    config: HashMap<String, String> = HashMap::new()
);

impl ArgoWorkflowsPlugin {
    async fn client(&self) -> PluginResult<&ArgoClient> {
        self.client
            .get_or_try_init(|| async {
                let source =
                    KubeconfigSource::from_config(&self.config, config::KUBECONFIG_PATH_KEY);
                let client =
                    kubernetes::connect(move || source.load(), config::context(&self.config), true)
                        .await?;
                Ok(ArgoClient::new(client))
            })
            .await
    }

    async fn workflows(
        &self, pipeline_id: &str,
    ) -> PluginResult<(config::PipelineRef, Vec<Workflow>)> {
        let pipeline = config::parse_pipeline_id(pipeline_id)?;
        let mut workflows = self.client().await?.list_workflows(&pipeline).await?;
        mapper::newest_first(&mut workflows);
        Ok((pipeline, workflows))
    }
}

plugin_impl!(ArgoWorkflowsPlugin [metadata, status_source, available_from_filtered] {
    fn initialize(
        &mut self, _provider_id: i64, config: HashMap<String, String>,
        _http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        self.status_source = custom_status_source(&config)?;
        self.config = config;
        self.client = OnceCell::new();
        Ok(())
    }

    fn configured_repositories(&self, config: &HashMap<String, String>) -> Vec<String> {
        config::selected(config)
            .into_iter()
            .map(|pipeline| format!("{}/{}", pipeline.namespace, pipeline.name))
            .collect()
    }

    fn run_display(&self, _pipeline: &PipelineFacts<'_>, run: &RunFacts<'_>) -> RunDisplay {
        let mut display = pipedash_plugin_api::plugin::defaults::run_display(run);
        if let Some(workflow) = run.metadata.get("workflow").and_then(serde_json::Value::as_str) {
            display.title = workflow.to_string();
        }
        display
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        self.configured_entries().await?;
        Ok(true)
    }

    async fn get_field_options(
        &self, field_key: &str, config: &HashMap<String, String>,
    ) -> PluginResult<Vec<String>> {
        if field_key != config::CONTEXT_KEY {
            return Ok(Vec::new());
        }
        let source = KubeconfigSource::from_config(config, config::KUBECONFIG_PATH_KEY);
        let description = source.describe();
        kubernetes::available_contexts(move || source.load(), description).await
    }

    async fn fetch_available_pipelines_filtered(
        &self, _org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let mut pipelines: Vec<AvailablePipeline> = self
            .configured_entries()
            .await?
            .iter()
            .map(|entry| mapper::map_available(&entry.pipeline))
            .collect();
        if let Some(search) = search {
            retain_matching(&mut pipelines, &search);
        }
        paginate(pipelines, params.unwrap_or_default())
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        self.fetch_pipelines_impl().await
    }

    async fn fetch_run_history(&self, pipeline_id: &str, limit: usize) -> PluginResult<Vec<PipelineRun>> {
        let (pipeline, workflows) = self.workflows(pipeline_id).await?;
        Ok(workflows
            .iter()
            .take(limit)
            .map(|workflow| mapper::map_run(&pipeline, workflow))
            .collect())
    }

    async fn fetch_run_details(&self, pipeline_id: &str, run_number: i64) -> PluginResult<PipelineRun> {
        let (pipeline, workflows) = self.workflows(pipeline_id).await?;
        workflows
            .iter()
            .find(|workflow| mapper::run_number(&workflow.metadata) == run_number)
            .map(|workflow| mapper::map_run(&pipeline, workflow))
            .ok_or_else(|| PluginError::PipelineNotFound(format!("Workflow #{run_number} not found")))
    }

    async fn trigger_pipeline(&self, _params: TriggerParams) -> PluginResult<TriggerResult> {
        Err(PluginError::NotSupported(
            "Argo Workflows runs can't be started from Pipedash".to_string(),
        ))
    }
});

#[cfg(test)]
mod tests;
