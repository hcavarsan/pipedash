use chrono::Utc;
use pipedash_plugin_api::schema::{
    ColumnDefinition,
    ColumnSort,
};

use super::filter::{
    facets,
    sort_runs,
    Criteria,
};
use crate::application::services::pipeline_service::PipelineService;
use crate::domain::pipeline_view::SortDirection;
use crate::domain::run_history::{
    AppliedRunSort,
    RunHistoryScanNotice,
    RunSort,
};
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
        let (value_column, sort) = self.resolve_sort(pipeline, query.sort.as_ref())?;
        let policy = self
            .provider_service
            .run_history_policy(&pipeline.provider_type);
        let prefix_sort = value_column.is_none() && sort.direction == SortDirection::Desc;
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
        let depth = query.page.max(query.scan_depth);
        let scan_budget = page_target.max(SCAN_LIMIT.saturating_mul(depth));
        loop {
            let enough_matches = prefix_sort
                && cached
                    .iter()
                    .filter(|run| criteria.matches(run))
                    .take(end.saturating_add(1))
                    .count()
                    > end;
            let fresh_enough = fetched || !policy.bounded;
            if (enough_matches && fresh_enough) || complete || stalled {
                break;
            }
            let limit = if policy.bounded {
                usize::MAX
            } else if !prefix_sort {
                scan_budget
            } else {
                page_target.max((cached.len() * 2 / FETCH_STEP + 1) * FETCH_STEP)
            };
            if !policy.bounded && cached.len() >= scan_budget {
                capped = true;
                break;
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
        }
        let exhausted = policy.bounded || complete || stalled;

        if !fetched && !policy.bounded && query.page == 1 {
            if let Ok(lease) = self.provider_service.acquire(pipeline.provider_id).await {
                self.spawn_metrics(lease, pipeline.id.clone(), None);
            }
        }

        let (branches, actors) = facets(&cached);
        let scan_notice = (!prefix_sort && !exhausted).then(|| RunHistoryScanNotice {
            title: "This sort needs the full history".to_string(),
            message: "The scan reached its limit before confirming the complete history."
                .to_string(),
            action_label: "Load more history".to_string(),
        });
        let sorted = if scan_notice.is_some() {
            Vec::new()
        } else {
            let matching: Vec<_> = cached
                .into_iter()
                .filter(|run| criteria.matches(run))
                .collect();
            match &value_column {
                Some(column) => sort_runs(matching, column, sort.direction),
                None if sort.direction == SortDirection::Asc => {
                    matching.into_iter().rev().collect()
                }
                None => matching,
            }
        };
        let total_count = sorted.len();
        Ok(RunHistoryPage {
            runs: sorted
                .into_iter()
                .skip(start)
                .take(query.page_size)
                .collect(),
            total_count,
            has_more: end < total_count,
            can_scan_deeper: capped && !exhausted,
            is_complete: exhausted,
            scan_notice,
            sort,
            page: query.page,
            page_size: query.page_size,
            total_pages: total_count.div_ceil(query.page_size),
            branches,
            actors,
        })
    }

    fn resolve_sort(
        &self, pipeline: &Pipeline, requested: Option<&RunSort>,
    ) -> DomainResult<(Option<ColumnDefinition>, AppliedRunSort)> {
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
        let Some(requested) = requested else {
            let column = table
                .columns
                .iter()
                .find(|column| column.sort == ColumnSort::HistoryOrder)
                .map(|column| column.id.clone());
            return Ok((
                None,
                AppliedRunSort {
                    column,
                    direction: SortDirection::Desc,
                },
            ));
        };
        let column = table
            .columns
            .iter()
            .find(|column| column.sort != ColumnSort::None && column.id == requested.column)
            .ok_or_else(|| {
                DomainError::InvalidConfig(format!(
                    "Cannot sort run history by '{}'",
                    requested.column
                ))
            })?;
        let applied = AppliedRunSort {
            column: Some(column.id.clone()),
            direction: requested.direction,
        };
        Ok((
            (column.sort == ColumnSort::Value).then(|| column.clone()),
            applied,
        ))
    }
}
