use pipedash_plugin_api::config::selected_items;
use pipedash_plugin_api::pagination::paginate;
use pipedash_plugin_api::support::try_buffered;
use pipedash_plugin_api::{
    AvailablePipeline,
    PaginatedResponse,
    PaginationParams,
    Pipeline,
    PipelineRun,
    PluginResult,
};

use super::{
    CircleCIPlugin,
    CONCURRENCY,
};
use crate::types::{
    CirclePipeline,
    Project,
    Workflow,
};
use crate::{
    config,
    mapper,
};

impl CircleCIPlugin {
    pub(super) async fn selected_projects(&self) -> PluginResult<Vec<Project>> {
        let client = self.client()?;
        let Some(selected) = selected_items(&self.config) else {
            return client.fetch_projects().await;
        };
        let requests: Vec<_> = selected
            .iter()
            .map(|slug| client.fetch_project(slug))
            .collect();
        try_buffered(requests, CONCURRENCY).await
    }

    pub(super) async fn to_run(
        &self, pipeline_id: &str, slug: &str, pipeline: &CirclePipeline,
    ) -> PluginResult<PipelineRun> {
        let workflows = self.client()?.fetch_workflows(&pipeline.id).await?;
        Ok(mapper::to_run(
            pipeline_id,
            pipeline,
            &workflows,
            &self.base_url,
            slug,
        ))
    }

    async fn runs(
        &self, pipeline_id: &str, slug: &str, pipelines: &[CirclePipeline],
    ) -> PluginResult<Vec<PipelineRun>> {
        let requests: Vec<_> = pipelines
            .iter()
            .map(|pipeline| self.to_run(pipeline_id, slug, pipeline))
            .collect();
        try_buffered(requests, CONCURRENCY).await
    }

    async fn latest_run(&self, project: &Project) -> PluginResult<Option<PipelineRun>> {
        let pipelines = self.client()?.fetch_pipelines(&project.slug, 1).await?;
        match pipelines.first() {
            Some(pipeline) => Ok(Some(
                self.to_run(&project.slug, &project.slug, pipeline).await?,
            )),
            None => Ok(None),
        }
    }

    pub(super) async fn run_workflows(
        &self, slug: &str, run_number: i64,
    ) -> PluginResult<(CirclePipeline, Vec<Workflow>)> {
        let client = self.client()?;
        let pipeline = client.fetch_pipeline(slug, run_number).await?;
        let workflows = client.fetch_workflows(&pipeline.id).await?;
        Ok((pipeline, workflows))
    }

    pub(super) async fn fetch_available_pipelines_filtered_impl(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let projects = self.client()?.fetch_projects().await?;
        let search = search.map(|term| term.to_lowercase());
        let matching: Vec<AvailablePipeline> = projects
            .iter()
            .map(mapper::to_available_pipeline)
            .filter(|project| org.is_none() || project.organization == org)
            .filter(|project| {
                search.as_ref().is_none_or(|term| {
                    project.name.to_lowercase().contains(term)
                        || project.id.to_lowercase().contains(term)
                })
            })
            .collect();

        paginate(matching, params.unwrap_or_default())
    }

    pub(super) async fn fetch_pipelines_impl(&self) -> PluginResult<Vec<Pipeline>> {
        let projects = self.selected_projects().await?;
        let requests: Vec<_> = projects
            .iter()
            .map(|project| self.latest_run(project))
            .collect();
        let latest: Vec<Option<PipelineRun>> = try_buffered(requests, CONCURRENCY).await?;

        let pipelines: Vec<Pipeline> = projects
            .iter()
            .zip(&latest)
            .map(|(project, run)| mapper::to_pipeline(project, run.as_ref()))
            .collect();
        tracing::debug!(
            pipelines = pipelines.len(),
            "Mapped CircleCI projects to pipelines"
        );
        Ok(pipelines)
    }

    pub(super) async fn fetch_run_history_impl(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let slug = config::parse_pipeline_id(pipeline_id)?;
        let pipelines = self.client()?.fetch_pipelines(slug, limit).await?;
        self.runs(pipeline_id, slug, &pipelines).await
    }

    pub(super) async fn fetch_run_details_impl(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let slug = config::parse_pipeline_id(pipeline_id)?;
        let pipeline = self.client()?.fetch_pipeline(slug, run_number).await?;
        self.to_run(pipeline_id, slug, &pipeline).await
    }
}
