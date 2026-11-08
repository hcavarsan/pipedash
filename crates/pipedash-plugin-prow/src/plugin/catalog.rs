use std::collections::HashMap;

use pipedash_plugin_api::pagination::paginate;
use pipedash_plugin_api::support::try_buffered;
use pipedash_plugin_api::{
    AvailablePipeline,
    PaginatedResponse,
    PaginationParams,
    PipelineRun,
    PluginError,
    PluginResult,
};

use super::ProwPlugin;
use crate::config::{
    self,
    Selection,
};
use crate::mapper;

const DEFAULT_PAGE_SIZE: usize = 100;
const HISTORY_CONCURRENCY: usize = 4;

impl ProwPlugin {
    pub(super) async fn fetch_available_pipelines_filtered_impl(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let deck = self.deck()?;
        let jobs = match org.as_deref() {
            Some(org) if org != deck.host() => deck.jobs(Some(org), None).await?,
            _ => deck.jobs(None, None).await?,
        };
        let search = search.map(|term| term.to_lowercase());
        let matching: Vec<AvailablePipeline> = mapper::catalog(&jobs, deck.host())
            .pipelines
            .into_iter()
            .filter(|pipeline| org.is_none() || pipeline.organization == org)
            .filter(|pipeline| {
                search.as_ref().is_none_or(|term| {
                    pipeline.name.to_lowercase().contains(term)
                        || pipeline
                            .repository
                            .as_ref()
                            .is_some_and(|repository| repository.to_lowercase().contains(term))
                })
            })
            .collect();

        let params = params.unwrap_or_default();
        paginate(
            matching,
            PaginationParams {
                page: params.page.max(1),
                page_size: if params.page_size == 0 {
                    DEFAULT_PAGE_SIZE
                } else {
                    params.page_size
                },
            },
        )
    }

    pub(super) async fn fetch_recent_runs_impl(
        &self, pipeline_ids: &[String], limit: usize,
    ) -> PluginResult<HashMap<String, Vec<PipelineRun>>> {
        let selections: Vec<(&String, Selection)> = pipeline_ids
            .iter()
            .filter_map(|id| {
                config::parse_pipeline_id(id)
                    .ok()
                    .map(|selection| (id, selection))
            })
            .collect();
        let jobs_by_scope = self
            .jobs_by_scope(
                selections
                    .iter()
                    .map(|(_, selection)| selection.scope.as_ref()),
            )
            .await?;

        let histories: Vec<_> = selections
            .iter()
            .map(|(id, selection)| {
                let jobs = jobs_by_scope
                    .get(&selection.scope.as_ref())
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                async move {
                    let runs = self.history_from(jobs, selection, id, limit).await?;
                    Ok::<_, PluginError>(((*id).clone(), runs))
                }
            })
            .collect();
        Ok(try_buffered(histories, HISTORY_CONCURRENCY)
            .await?
            .into_iter()
            .collect())
    }
}
