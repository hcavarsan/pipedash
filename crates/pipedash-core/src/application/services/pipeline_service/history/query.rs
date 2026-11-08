use chrono::Utc;
use pipedash_plugin_api::schema::ColumnDefinition;

use super::filter::{
    facets,
    sort_runs,
    Criteria,
};
use crate::application::services::pipeline_service::PipelineService;
use crate::domain::{
    DomainError,
    DomainResult,
    PaginationParams,
    Pipeline,
    RunHistoryPage,
    RunHistoryQuery,
};

const ALL_RUNS: usize = i64::MAX as usize;
const FETCH_STEP: usize = 100;
const SCAN_LIMIT: usize = 500;
const RUNS_TABLE: &str = "pipeline_runs";

impl PipelineService {
    pub async fn query_run_history(
        &self, pipeline_id: &str, query: RunHistoryQuery,
    ) -> DomainResult<RunHistoryPage> {
        let pipeline = self.repository.get_cached_pipeline(pipeline_id).await?;
        let mut page = self.run_history_page(&pipeline, &query).await?;
        self.presenter.runs(&pipeline, &mut page.runs).await?;
        Ok(page)
    }

    pub(crate) async fn run_history_page(
        &self, pipeline: &Pipeline, query: &RunHistoryQuery,
    ) -> DomainResult<RunHistoryPage> {
        let params = PaginationParams {
            page: query.page,
            page_size: query.page_size,
        };
        params.validate().map_err(DomainError::InvalidConfig)?;
        let start = params
            .calculate_offset()
            .map_err(DomainError::InvalidConfig)?;
        let end = start
            .checked_add(query.page_size)
            .ok_or_else(|| DomainError::InvalidConfig("Pagination offset overflow".to_string()))?;
        let column = self.sort_column(pipeline, &query.sort_column)?;
        let policy = self
            .provider_service
            .run_history_policy(&pipeline.provider_type);
        let criteria = Criteria::new(query, Utc::now());
        let page_target = (end / FETCH_STEP + 1) * FETCH_STEP;

        let mut cached = self
            .repository
            .get_cached_run_history(&pipeline.id, ALL_RUNS, policy.order)
            .await?;
        let mut complete = false;
        let mut stalled = false;
        let mut capped = false;
        let mut fetched = false;
        let scan_budget = page_target.max(SCAN_LIMIT.saturating_mul(query.page));
        let matched = loop {
            let matched: Vec<_> = cached
                .iter()
                .filter(|run| criteria.matches(run))
                .cloned()
                .collect();
            let fresh_enough = fetched || !policy.bounded;
            if (matched.len() > end && fresh_enough) || complete || stalled {
                break matched;
            }
            let limit = if policy.bounded {
                usize::MAX
            } else {
                page_target.max((cached.len() * 2 / FETCH_STEP + 1) * FETCH_STEP)
            };
            if !policy.bounded && cached.len() >= scan_budget {
                capped = true;
                break matched;
            }
            let limit = if policy.bounded {
                limit
            } else {
                limit.min(scan_budget)
            };

            let before = cached.len();
            let (api_runs, lease) = self.fetch_run_history_leased(pipeline, limit).await?;
            fetched = true;
            complete = api_runs.len() < limit;
            self.spawn_metrics(lease, pipeline.id.clone(), Some(api_runs));
            cached = self
                .repository
                .get_cached_run_history(&pipeline.id, ALL_RUNS, policy.order)
                .await?;
            stalled = !complete && cached.len() <= before;
        };
        let exhausted = policy.bounded || complete || stalled;

        if !fetched && !policy.bounded && query.page == 1 {
            if let Ok(lease) = self.provider_service.acquire(pipeline.provider_id).await {
                self.spawn_metrics(lease, pipeline.id.clone(), None);
            }
        }

        let (branches, actors) = facets(&cached);
        let sorted = sort_runs(matched, &column, query.direction);
        let total_count = sorted.len();
        Ok(RunHistoryPage {
            runs: sorted
                .into_iter()
                .skip(start)
                .take(query.page_size)
                .collect(),
            total_count,
            has_more: end < total_count || (capped && !exhausted),
            is_complete: exhausted,
            page: query.page,
            page_size: query.page_size,
            total_pages: total_count.div_ceil(query.page_size),
            branches,
            actors,
        })
    }

    fn sort_column(&self, pipeline: &Pipeline, requested: &str) -> DomainResult<ColumnDefinition> {
        let plugin = self
            .provider_service
            .plugin_registry()
            .get(&pipeline.provider_type)
            .ok_or_else(|| DomainError::InvalidProviderType(pipeline.provider_type.clone()))?;
        let table = plugin
            .metadata()
            .table_schema
            .get_table(RUNS_TABLE)
            .ok_or_else(|| DomainError::NotSupported("Run history has no table schema".into()))?;
        let id = Some(requested)
            .filter(|id| !id.is_empty())
            .or(table.default_sort_column.as_deref())
            .unwrap_or("run_number");
        table
            .columns
            .iter()
            .find(|column| column.sortable && column.id == id)
            .cloned()
            .ok_or_else(|| DomainError::InvalidConfig(format!("Cannot sort run history by '{id}'")))
    }
}
