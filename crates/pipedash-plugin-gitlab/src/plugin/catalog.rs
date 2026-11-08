use pipedash_plugin_api::config::selected_items;
use pipedash_plugin_api::support::join_buffered;
use pipedash_plugin_api::utils::collect_partial;
use pipedash_plugin_api::*;

use super::GitLabPlugin;
use crate::{
    config,
    mapper,
    types,
};

pub(super) fn namespace_of(project: &types::Project) -> Option<String> {
    let parts: Vec<&str> = project.name_with_namespace.split('/').collect();
    (parts.len() >= 2).then(|| parts[..parts.len() - 1].join("/"))
}

pub(super) fn available_page(
    response: PaginatedResponse<types::Project>,
) -> PaginatedResponse<AvailablePipeline> {
    PaginatedResponse::new(
        response
            .items
            .iter()
            .map(mapper::map_available_pipeline)
            .collect(),
        response.page,
        response.page_size,
        response.total_count,
    )
}

impl GitLabPlugin {
    pub(super) async fn fetch_all_projects(&self) -> PluginResult<Vec<types::Project>> {
        let client = self.client()?;
        let mut all_projects = Vec::new();
        let mut page = 1;

        loop {
            let params = PaginationParams {
                page,
                page_size: 100,
            };
            let response = client.list_projects(&params).await?;
            if response.items.is_empty() {
                break;
            }
            all_projects.extend(response.items);

            if !response.has_more || page > 100 {
                break;
            }
            page += 1;
        }

        if let Some(selected_paths) = selected_items(&self.config) {
            Ok(all_projects
                .into_iter()
                .filter(|p| {
                    let normalized = p.name_with_namespace.replace(" ", "");
                    selected_paths.contains(&normalized)
                })
                .collect())
        } else {
            Ok(all_projects)
        }
    }

    pub(super) async fn fetch_projects_paginated(
        &self, params: &PaginationParams,
    ) -> PluginResult<PaginatedResponse<types::Project>> {
        let client = self.client()?;
        let response = client.list_projects(params).await?;

        if let Some(selected_paths) = selected_items(&self.config) {
            let filtered_items: Vec<_> = response
                .items
                .into_iter()
                .filter(|p| {
                    let normalized = p.name_with_namespace.replace(" ", "");
                    selected_paths.contains(&normalized)
                })
                .collect();
            let filtered_count = filtered_items.len();
            Ok(PaginatedResponse::new(
                filtered_items,
                params.page,
                params.page_size,
                filtered_count,
            ))
        } else {
            Ok(response)
        }
    }
}

impl GitLabPlugin {
    pub(super) async fn fetch_pipelines_impl(&self) -> PluginResult<Vec<Pipeline>> {
        let projects = self.fetch_all_projects().await?;

        let requests: Vec<_> = projects
            .iter()
            .map(|project| self.project_pipeline(project))
            .collect();
        let results: Vec<_> = join_buffered(requests, 8).await;

        Ok(collect_partial(
            results,
            |error| tracing::warn!(%error, item = "project", "Skipping failed GitLab item"),
        )?
        .into_iter()
        .flatten()
        .collect())
    }

    async fn project_pipeline(&self, project: &types::Project) -> PluginResult<Option<Pipeline>> {
        let client = self.client()?;
        let pipelines = client.get_project_pipelines(project.id, 1).await?;
        let latest_pipeline = pipelines.first();
        if latest_pipeline.is_none() && !client.has_ci_config(project).await.unwrap_or(true) {
            return Ok(None);
        }
        Ok(Some(mapper::map_pipeline(project, latest_pipeline)))
    }

    pub(super) async fn fetch_run_history_impl(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let project_id = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;

        let project = client.get_project(project_id).await?;

        let pipeline_list = client.get_project_pipelines(project_id, limit).await?;

        let namespace = namespace_of(&project);

        let requests: Vec<_> = pipeline_list
            .iter()
            .map(|p| client.get_pipeline(project_id, p.id))
            .collect();
        let detailed_results: Vec<_> = join_buffered(requests, 8).await;
        let detailed_pipelines = collect_partial(
            detailed_results,
            |error| tracing::warn!(%error, item = "pipeline run", "Skipping failed GitLab item"),
        )?;

        Ok(detailed_pipelines
            .into_iter()
            .map(|p| mapper::map_pipeline_run(&p, project_id, namespace.as_deref()))
            .collect())
    }
}
