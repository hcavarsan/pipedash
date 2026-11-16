use std::collections::HashSet;

use futures::stream::{
    self,
    StreamExt,
};

use super::PipelineService;
use crate::domain::{
    CancelGroup,
    CancellationBatch,
    CancellationResult,
    CancellationTargets,
    DomainError,
    DomainResult,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    RunHistoryPage,
    RunHistoryQuery,
    RunTarget,
    SkippedPipeline,
};

const SCAN_CONCURRENCY: usize = 4;
const SCAN_PAGE_SIZE: usize = 100;
const MAX_TARGETS: usize = 1000;
const NOT_CANCELLABLE: &str = "it can't cancel runs from Pipedash";

#[derive(Default)]
struct Scanned {
    group: Option<CancelGroup>,
    skipped: Option<SkippedPipeline>,
    failed: Option<String>,
}

fn unique<T: Clone + std::hash::Hash + Eq>(items: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|item| seen.insert(item.clone()))
        .collect()
}

fn is_unfinished(status: &PipelineStatus) -> bool {
    matches!(status, PipelineStatus::Running | PipelineStatus::Pending)
}

fn next_scan(history: &RunHistoryPage, page: usize, scan_depth: usize) -> Option<(usize, usize)> {
    if history.has_more {
        return Some((page + 1, scan_depth));
    }
    if !history.can_scan_deeper {
        return None;
    }
    if history.runs.len() < history.page_size {
        return Some((page, scan_depth + 1));
    }
    Some((page + 1, scan_depth + 1))
}

impl PipelineService {
    pub async fn cancellation_targets(
        &self, pipeline_ids: Vec<String>,
    ) -> DomainResult<CancellationTargets> {
        let scans: Vec<Scanned> = stream::iter(unique(pipeline_ids))
            .map(|id| async move { self.scan_unfinished(&id).await })
            .buffered(SCAN_CONCURRENCY)
            .collect()
            .await;

        let mut targets = CancellationTargets::default();
        for scan in scans {
            targets.skipped.extend(scan.skipped);
            targets.failed_pipeline_ids.extend(scan.failed);
            if let Some(group) = scan.group {
                targets.run_count += group.runs.len();
                for run in &group.runs {
                    if targets
                        .batches
                        .last()
                        .is_none_or(|batch| batch.len() == MAX_TARGETS)
                    {
                        targets.batches.push(Vec::new());
                    }
                    if let Some(batch) = targets.batches.last_mut() {
                        batch.push(RunTarget {
                            pipeline_id: group.pipeline.id.clone(),
                            run_number: run.run_number,
                        });
                    }
                }
                targets.groups.push(group);
            }
        }
        Ok(targets)
    }

    pub async fn cancel_runs(&self, targets: Vec<RunTarget>) -> DomainResult<CancellationBatch> {
        if targets.len() > MAX_TARGETS {
            return Err(DomainError::InvalidConfig(format!(
                "Cancel at most {MAX_TARGETS} runs at a time"
            )));
        }

        let results: Vec<CancellationResult> = stream::iter(unique(targets))
            .map(|target| self.cancel_target(target))
            .buffered(SCAN_CONCURRENCY)
            .collect()
            .await;
        Ok(CancellationBatch {
            requested_count: results.len(),
            failed_count: results.iter().filter(|r| r.error.is_some()).count(),
            results,
        })
    }

    async fn cancel_target(&self, target: RunTarget) -> CancellationResult {
        let pipeline_name = match self
            .repository
            .get_cached_pipeline(&target.pipeline_id)
            .await
        {
            Ok(pipeline) => pipeline.name,
            Err(_) => target.pipeline_id.clone(),
        };
        let error = self
            .cancel_run(&target.pipeline_id, target.run_number)
            .await
            .err()
            .map(|error| error.to_string());

        CancellationResult {
            target,
            pipeline_name,
            error,
        }
    }

    async fn scan_unfinished(&self, pipeline_id: &str) -> Scanned {
        let Ok(pipeline) = self.presented_pipeline(pipeline_id).await else {
            return Scanned {
                failed: Some(pipeline_id.to_string()),
                ..Scanned::default()
            };
        };
        let provider_name = self
            .provider_service
            .plugin_registry()
            .get(&pipeline.provider_type)
            .map(|plugin| plugin.metadata().name.clone())
            .unwrap_or_default();

        if !pipeline.actions.cancel.allowed {
            return Scanned {
                skipped: is_unfinished(&pipeline.status).then(|| SkippedPipeline {
                    pipeline_id: pipeline.id.clone(),
                    provider_name,
                    reason: pipeline
                        .actions
                        .cancel
                        .reason
                        .clone()
                        .unwrap_or_else(|| NOT_CANCELLABLE.to_string()),
                }),
                ..Scanned::default()
            };
        }

        let (runs, complete) = self.unfinished_runs(&pipeline).await;
        Scanned {
            group: (!runs.is_empty()).then(|| CancelGroup {
                pipeline: pipeline.clone(),
                provider_name,
                runs,
            }),
            failed: (!complete).then(|| pipeline.id.clone()),
            ..Scanned::default()
        }
    }

    async fn unfinished_runs(&self, pipeline: &Pipeline) -> (Vec<PipelineRun>, bool) {
        let mut runs = Vec::new();
        let (mut page, mut scan_depth) = (1, 1);
        loop {
            let query = RunHistoryQuery {
                page,
                page_size: SCAN_PAGE_SIZE,
                statuses: vec![PipelineStatus::Running, PipelineStatus::Pending],
                scan_depth,
                ..RunHistoryQuery::default()
            };
            let history = match self.run_history_page(pipeline, &query).await {
                Ok(history) => history,
                Err(error) => {
                    tracing::warn!(pipeline = %pipeline.id, error = %error, "Failed to scan unfinished runs");
                    return (runs, false);
                }
            };
            let (next_page, next_depth) = match next_scan(&history, page, scan_depth) {
                Some(next) => next,
                None => {
                    runs.extend(history.runs);
                    break;
                }
            };
            if next_page > page {
                runs.extend(history.runs);
            }
            (page, scan_depth) = (next_page, next_depth);
        }

        if let Err(error) = self.presenter.runs(pipeline, &mut runs).await {
            tracing::warn!(pipeline = %pipeline.id, error = %error, "Failed to present unfinished runs");
            return (Vec::new(), false);
        }
        runs.retain(|run| run.actions.cancel.allowed);
        (runs, true)
    }
}

#[cfg(test)]
mod tests;
