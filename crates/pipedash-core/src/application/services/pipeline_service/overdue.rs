use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    RunHistoryOrder,
    RunHistoryPolicy,
};
use tokio::sync::Semaphore;

use super::{
    PipelineService,
    RunTiming,
    OVERDUE_CONCURRENT_CHECKS,
    OVERDUE_RECENT_RUNS,
};
use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    RunHistoryQuery,
};

const OVERDUE_FACTOR: i64 = 3;
const OVERDUE_MIN_SUCCESSFUL_RUNS: usize = 3;
const OVERDUE_HISTORY_FETCH: usize = 100;

impl PipelineService {
    pub async fn get_cached_pipelines(
        &self, provider_id: Option<i64>,
    ) -> DomainResult<Vec<Pipeline>> {
        let mut pipelines = self.cached_pipelines(provider_id).await?;
        self.presenter.pipelines(&mut pipelines).await?;
        Ok(pipelines)
    }

    pub(crate) async fn cached_pipelines(
        &self, provider_id: Option<i64>,
    ) -> DomainResult<Vec<Pipeline>> {
        Ok(self.mark_overdue(self.repository.get_cached_pipelines(provider_id).await?))
    }

    pub(super) fn mark_overdue(&self, mut pipelines: Vec<Pipeline>) -> Vec<Pipeline> {
        for pipeline in &mut pipelines {
            let timing = (pipeline.status == PipelineStatus::Running)
                .then(|| self.run_timings.get(&pipeline.id))
                .flatten();
            pipeline.overdue_run = timing.as_ref().and_then(|t| t.overdue.clone());
            pipeline.typical_duration_seconds = timing.as_ref().map(|t| t.typical);
        }

        pipelines
    }

    pub async fn update_overdue_pipelines(&self, provider_id: i64) -> bool {
        let pipelines = match self
            .repository
            .get_cached_pipelines(Some(provider_id))
            .await
        {
            Ok(pipelines) => pipelines,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load pipelines for the overdue check");
                return false;
            }
        };
        let now = Utc::now();
        let policy = pipelines
            .first()
            .map(|p| self.provider_service.run_history_policy(&p.provider_type))
            .unwrap_or_default();
        let tracked: Vec<&Pipeline> = pipelines
            .iter()
            .filter(|p| tracks_overdue(p, policy))
            .collect();
        let prefetched = self.prefetch_recent_runs(provider_id, &tracked).await;
        let permits = Semaphore::new(OVERDUE_CONCURRENT_CHECKS);
        let checks = tracked.into_iter().map(|pipeline| {
            let permits = &permits;
            let prefetched = &prefetched;
            async move {
                let runs = self
                    .runs_for_overdue(&pipeline.id, policy.order, prefetched, permits)
                    .await?;
                typical_duration(&runs).map(|typical| {
                    (
                        pipeline.id.clone(),
                        RunTiming {
                            typical,
                            overdue: overdue_run(&runs, now),
                        },
                    )
                })
            }
        });
        let timings: HashMap<String, RunTiming> = futures::future::join_all(checks)
            .await
            .into_iter()
            .flatten()
            .collect();

        let mut changed = false;
        self.run_timings.retain(|id, timing| {
            let keep = provider_id_of(id) != Some(provider_id) || timings.get(id) == Some(timing);
            changed |= !keep;
            keep
        });
        for (id, timing) in timings {
            changed |= self.run_timings.insert(id, timing).is_none();
        }

        changed
    }

    async fn runs_for_overdue(
        &self, pipeline_id: &str, order: RunHistoryOrder,
        prefetched: &HashMap<String, Option<Vec<PipelineRun>>>, permits: &Semaphore,
    ) -> Option<Vec<PipelineRun>> {
        if let Some(batched) = prefetched.get(pipeline_id) {
            return batched.clone();
        }
        let _permit = permits.acquire().await.ok()?;
        match self.recent_runs(pipeline_id, order).await {
            Ok(runs) => Some(runs),
            Err(e) => {
                tracing::debug!(pipeline_id = %pipeline_id, error = %e, "Overdue check failed");
                None
            }
        }
    }

    async fn prefetch_recent_runs(
        &self, provider_id: i64, pipelines: &[&Pipeline],
    ) -> HashMap<String, Option<Vec<PipelineRun>>> {
        let mut uncached = Vec::new();
        for pipeline in pipelines {
            if matches!(
                self.repository.get_cached_run_count(&pipeline.id).await,
                Ok(0)
            ) {
                uncached.push(*pipeline);
            }
        }
        if uncached.is_empty() {
            return HashMap::new();
        }
        let Ok(lease) = self.provider_service.acquire(provider_id).await else {
            return HashMap::new();
        };
        let ids: Vec<String> = uncached.iter().map(|p| p.id.clone()).collect();
        let mut fetched = match lease
            .provider
            .fetch_recent_runs(&ids, OVERDUE_HISTORY_FETCH)
            .await
        {
            Ok(fetched) => fetched,
            Err(DomainError::NotSupported(_)) => return HashMap::new(),
            Err(e) => {
                tracing::debug!(provider_id, error = %e, "Batched overdue check failed");
                return ids.into_iter().map(|id| (id, None)).collect();
            }
        };

        let mut prefetched = HashMap::new();
        for pipeline in uncached {
            let runs = fetched.remove(&pipeline.id);
            if let Some(api_runs) = &runs {
                if let Some(Err(e)) = self
                    .provider_service
                    .commit(&lease, || {
                        self.merge_run_cache(lease.provider.as_ref(), pipeline, api_runs)
                    })
                    .await
                {
                    tracing::debug!(pipeline_id = %pipeline.id, error = %e, "Failed to cache batched runs");
                }
                self.spawn_metrics(lease.clone(), pipeline.id.clone(), Some(api_runs.clone()));
            }
            prefetched.insert(
                pipeline.id.clone(),
                runs.map(|runs| runs.into_iter().take(OVERDUE_RECENT_RUNS).collect()),
            );
        }
        prefetched
    }

    async fn recent_runs(
        &self, pipeline_id: &str, order: RunHistoryOrder,
    ) -> DomainResult<Vec<PipelineRun>> {
        if self.repository.get_cached_run_count(pipeline_id).await? > 0 {
            return self
                .repository
                .get_paginated_runs(pipeline_id, 1, OVERDUE_RECENT_RUNS, order)
                .await;
        }

        let pipeline = self.repository.get_cached_pipeline(pipeline_id).await?;
        Ok(self
            .run_history_page(&pipeline, &RunHistoryQuery::page(1, OVERDUE_RECENT_RUNS))
            .await?
            .runs)
    }
}

pub(crate) fn provider_id_of(pipeline_id: &str) -> Option<i64> {
    pipeline_id.split("__").nth(1)?.parse().ok()
}

pub(crate) fn tracks_overdue(pipeline: &Pipeline, policy: RunHistoryPolicy) -> bool {
    pipeline.status == PipelineStatus::Running && policy.tracks_overdue
}

pub(crate) fn overdue_run(runs: &[PipelineRun], now: DateTime<Utc>) -> Option<String> {
    let running = runs
        .iter()
        .find(|run| run.status == PipelineStatus::Running)?;
    let median = typical_duration(runs)?;

    ((now - running.started_at).num_seconds() > OVERDUE_FACTOR * median).then(|| running.id.clone())
}

pub(crate) fn typical_duration(runs: &[PipelineRun]) -> Option<i64> {
    let mut durations: Vec<i64> = runs
        .iter()
        .filter(|run| run.status == PipelineStatus::Success)
        .filter_map(|run| run.duration_seconds)
        .filter(|seconds| *seconds > 0)
        .collect();
    if durations.len() < OVERDUE_MIN_SUCCESSFUL_RUNS {
        return None;
    }
    durations.sort_unstable();
    let middle = durations.len() / 2;

    Some(if durations.len() % 2 == 0 {
        (durations[middle - 1] + durations[middle]) / 2
    } else {
        durations[middle]
    })
}
