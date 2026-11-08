use pipedash_plugin_api::pagination::paginate;
use pipedash_plugin_api::support::{
    retain_matching,
    try_buffered,
};
use pipedash_plugin_api::{
    AvailablePipeline,
    PaginatedResponse,
    PaginationParams,
    Pipeline,
    PluginError,
    PluginResult,
};

use super::{
    TektonPlugin,
    CONCURRENCY,
};
use crate::{
    config,
    mapper,
    types,
};

impl TektonPlugin {
    async fn fetch_all_pipelines_in_namespaces(&self) -> PluginResult<Vec<types::TektonPipeline>> {
        let client = self.client().await?;

        let selected_ids = config::get_selected_pipelines(&self.config);

        let namespaces = if selected_ids.is_empty() {
            let namespace_mode = config::get_namespace_mode(&self.config);

            match namespace_mode {
                config::NamespaceMode::Custom => config::get_namespaces(&self.config),
                config::NamespaceMode::All => client.list_namespaces_with_pipelines().await?,
            }
        } else {
            let unique_namespaces: std::collections::HashSet<String> = selected_ids
                .iter()
                .filter_map(|id| {
                    id.split_once("__")
                        .map(|(namespace, _)| namespace.to_string())
                })
                .collect();
            unique_namespaces.into_iter().collect()
        };

        let requests: Vec<_> = namespaces
            .iter()
            .map(|namespace| client.list_pipelines(namespace))
            .collect();
        let all_pipelines: Vec<types::TektonPipeline> = try_buffered(requests, CONCURRENCY)
            .await?
            .into_iter()
            .flatten()
            .collect();

        if selected_ids.is_empty() {
            Ok(all_pipelines)
        } else {
            Ok(all_pipelines
                .into_iter()
                .filter(|p| {
                    let id = format!("{}__{}", p.metadata.namespace, p.metadata.name);
                    selected_ids.contains(&id)
                })
                .collect())
        }
    }

    async fn fetch_latest_run_for_pipeline(
        &self, namespace: &str, pipeline_name: &str,
    ) -> PluginResult<Option<types::TektonPipelineRun>> {
        let client = self.client().await?;
        let mut runs = match client
            .list_pipelineruns(namespace, Some(pipeline_name))
            .await
        {
            Ok(runs) => runs,
            Err(PluginError::PipelineNotFound(message)) => {
                tracing::warn!(namespace, pipeline_name, %message, "PipelineRuns not found for pipeline");
                return Ok(None);
            }
            Err(error) => return Err(error),
        };

        runs.sort_by(|a, b| {
            let a_time = types::parse_timestamp(&a.metadata.creation_timestamp);
            let b_time = types::parse_timestamp(&b.metadata.creation_timestamp);
            b_time.cmp(&a_time)
        });

        Ok(runs.into_iter().next())
    }

    pub(super) async fn fetch_pipelines_impl(&self) -> PluginResult<Vec<Pipeline>> {
        let pipelines = self.fetch_all_pipelines_in_namespaces().await?;

        let requests: Vec<_> = pipelines
            .iter()
            .map(|pipeline| async move {
                let latest_run = self
                    .fetch_latest_run_for_pipeline(
                        &pipeline.metadata.namespace,
                        &pipeline.metadata.name,
                    )
                    .await?;
                Ok(mapper::map_pipeline(pipeline, latest_run.as_ref()))
            })
            .collect();
        try_buffered(requests, CONCURRENCY).await
    }

    pub(super) async fn fetch_available_pipelines_filtered_impl(
        &self, _org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let pipelines = self.fetch_all_pipelines_in_namespaces().await?;
        let mut all_pipelines: Vec<_> = pipelines
            .iter()
            .map(mapper::map_available_pipeline)
            .collect();

        if let Some(search_term) = search {
            retain_matching(&mut all_pipelines, &search_term);
        }

        paginate(all_pipelines, params.unwrap_or_default())
    }
}
