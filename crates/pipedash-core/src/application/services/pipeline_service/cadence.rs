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
    DomainResult,
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
    failed_at: Option<DateTime<Utc>>,
}

pub(super) struct StagedGaps {
    provider_id: i64,
    current: HashSet<String>,
    learned: Vec<(String, LearnedGap)>,
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

    pub(super) async fn stage_provider_gaps(&self, provider_id: i64) -> Option<StagedGaps> {
        let pipelines = match self
            .repository
            .get_cached_pipelines(Some(provider_id))
            .await
        {
            Ok(pipelines) => pipelines,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load pipelines for the cadence check");
                return None;
            }
        };
        let order = pipelines
            .first()
            .map(|p| self.provider_service.run_history_policy(&p.provider_type))
            .unwrap_or_default()
            .order;

        Some(StagedGaps {
            provider_id,
            current: pipelines.iter().map(|p| p.id.clone()).collect(),
            learned: self.learn_gaps(&pipelines, order, Utc::now()).await,
        })
    }

    pub(super) fn apply_gaps(&self, staged: StagedGaps) -> bool {
        let StagedGaps {
            provider_id,
            current,
            learned,
        } = staged;
        self.run_gaps
            .retain(|id, _| provider_id_of(id) != Some(provider_id) || current.contains(id));
        let mut changed = false;
        for (id, next) in learned {
            let was_quiet = self.run_gaps.get(&id).is_some_and(|known| known.quiet);
            changed |= was_quiet != next.quiet;
            self.run_gaps.insert(id, next);
        }

        changed
    }

    async fn learn_gaps(
        &self, pipelines: &[Pipeline], order: RunHistoryOrder, now: DateTime<Utc>,
    ) -> Vec<(String, LearnedGap)> {
        let mut fetches_left = CADENCE_FETCHES_PER_PASS;
        let mut learned = Vec::new();
        let mut pending: Vec<&Pipeline> = pipelines
            .iter()
            .filter(|pipeline| pipeline.last_run.is_some())
            .collect();
        pending.sort_by_cached_key(|pipeline| {
            self.run_gaps
                .get(&pipeline.id)
                .and_then(|known| known.failed_at)
        });
        for pipeline in pending {
            let known = self.run_gaps.get(&pipeline.id).map(|entry| entry.clone());
            let usual_gap_seconds = match &known {
                Some(known) if known.last_run == pipeline.last_run => known.usual_gap_seconds,
                Some(LearnedGap {
                    usual_gap_seconds: Some(gap),
                    ..
                }) => self
                    .learned_gap(&pipeline.id, order, false)
                    .await
                    .ok()
                    .flatten()
                    .flatten()
                    .or(Some(*gap)),
                _ if fetches_left > 0 => {
                    fetches_left -= 1;
                    match self.learned_gap(&pipeline.id, order, true).await {
                        Ok(Some(gap)) => gap,
                        Ok(None) => continue,
                        Err(_) => {
                            learned.push((pipeline.id.clone(), failed_fetch(now)));
                            continue;
                        }
                    }
                }
                _ => continue,
            };
            learned.push((
                pipeline.id.clone(),
                LearnedGap {
                    usual_gap_seconds,
                    last_run: pipeline.last_run,
                    quiet: usual_gap_seconds
                        .is_some_and(|gap| is_quiet(pipeline.last_run, gap, now)),
                    failed_at: None,
                },
            ));
        }

        learned
    }

    async fn learned_gap(
        &self, pipeline_id: &str, order: RunHistoryOrder, may_fetch: bool,
    ) -> DomainResult<Option<Option<i64>>> {
        let cached = self.repository.get_cached_run_count(pipeline_id).await? > 0;
        if !cached && !may_fetch {
            return Ok(Some(None));
        }
        let runs = self
            .recent_runs(pipeline_id, order)
            .await
            .inspect_err(|e| {
                tracing::debug!(pipeline_id = %pipeline_id, error = %e, "Cadence check failed");
            })?;
        Ok(runs.map(|runs| usual_gap(&runs)))
    }
}

fn failed_fetch(now: DateTime<Utc>) -> LearnedGap {
    LearnedGap {
        usual_gap_seconds: None,
        last_run: None,
        quiet: false,
        failed_at: Some(now),
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
