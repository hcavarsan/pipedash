use std::collections::HashSet;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::RunHistoryOrder;

use super::overdue::{
    median,
    provider_id_of,
};
use super::{
    PipelineService,
    OVERDUE_RECENT_RUNS,
};
use crate::domain::{
    Pipeline,
    PipelineCadence,
    PipelineRun,
};

const CADENCE_MIN_GAPS: usize = 3;
const CADENCE_FETCHES_PER_PASS: usize = 4;
const QUIET_FACTOR: i64 = 2;

#[derive(Clone, PartialEq)]
pub(super) struct LearnedGap {
    usual_gap_seconds: Option<i64>,
    last_run: Option<DateTime<Utc>>,
    quiet: bool,
}

impl PipelineService {
    pub(super) fn cadence_of(
        &self, pipeline: &Pipeline, now: DateTime<Utc>,
    ) -> Option<PipelineCadence> {
        let usual_gap_seconds = self.run_gaps.get(&pipeline.id)?.usual_gap_seconds?;
        Some(PipelineCadence {
            usual_gap_seconds,
            quiet: is_quiet(pipeline.last_run, usual_gap_seconds, now),
        })
    }

    pub(super) async fn update_provider_gaps(&self, provider_id: i64) -> bool {
        let pipelines = match self
            .repository
            .get_cached_pipelines(Some(provider_id))
            .await
        {
            Ok(pipelines) => pipelines,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load pipelines for the cadence check");
                return false;
            }
        };
        let order = pipelines
            .first()
            .map(|p| self.provider_service.run_history_policy(&p.provider_type))
            .unwrap_or_default()
            .order;
        let current: HashSet<&str> = pipelines.iter().map(|p| p.id.as_str()).collect();
        self.run_gaps.retain(|id, _| {
            provider_id_of(id) != Some(provider_id) || current.contains(id.as_str())
        });

        self.update_run_gaps(&pipelines, order, Utc::now()).await
    }

    pub(super) async fn update_run_gaps(
        &self, pipelines: &[Pipeline], order: RunHistoryOrder, now: DateTime<Utc>,
    ) -> bool {
        let mut fetches_left = CADENCE_FETCHES_PER_PASS;
        let mut changed = false;
        for pipeline in pipelines
            .iter()
            .filter(|pipeline| pipeline.last_run.is_some())
        {
            let known = self.run_gaps.get(&pipeline.id).map(|entry| entry.clone());
            let usual_gap_seconds = match &known {
                Some(known) if known.last_run == pipeline.last_run => known.usual_gap_seconds,
                Some(known) => self
                    .learned_gap(&pipeline.id, order, false)
                    .await
                    .or(known.usual_gap_seconds),
                None if fetches_left > 0 => {
                    fetches_left -= 1;
                    self.learned_gap(&pipeline.id, order, true).await
                }
                None => continue,
            };
            let next = LearnedGap {
                usual_gap_seconds,
                last_run: pipeline.last_run,
                quiet: usual_gap_seconds.is_some_and(|gap| is_quiet(pipeline.last_run, gap, now)),
            };
            changed |= known.is_some_and(|known| known.quiet) != next.quiet;
            self.run_gaps.insert(pipeline.id.clone(), next);
        }

        changed
    }

    async fn learned_gap(
        &self, pipeline_id: &str, order: RunHistoryOrder, may_fetch: bool,
    ) -> Option<i64> {
        let cached = self
            .repository
            .get_cached_run_count(pipeline_id)
            .await
            .ok()?
            > 0;
        if !cached && !may_fetch {
            return None;
        }
        match self.recent_runs(pipeline_id, order).await {
            Ok(runs) => usual_gap(&runs),
            Err(e) => {
                tracing::debug!(pipeline_id = %pipeline_id, error = %e, "Cadence check failed");
                None
            }
        }
    }
}

pub(super) fn usual_gap(runs: &[PipelineRun]) -> Option<i64> {
    let mut starts: Vec<DateTime<Utc>> = runs
        .iter()
        .take(OVERDUE_RECENT_RUNS)
        .map(|run| run.started_at)
        .collect();
    starts.sort_unstable();
    starts.dedup();
    let gaps: Vec<i64> = starts
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).num_seconds())
        .filter(|seconds| *seconds > 0)
        .collect();

    median(gaps, CADENCE_MIN_GAPS)
}

pub(super) fn is_quiet(
    last_run: Option<DateTime<Utc>>, usual_gap_seconds: i64, now: DateTime<Utc>,
) -> bool {
    last_run
        .is_some_and(|last_run| (now - last_run).num_seconds() > QUIET_FACTOR * usual_gap_seconds)
}

#[cfg(test)]
mod tests;
