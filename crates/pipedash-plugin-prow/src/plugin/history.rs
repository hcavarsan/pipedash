use std::collections::{
    HashMap,
    HashSet,
};

use chrono::Utc;
use pipedash_plugin_api::support::try_buffered;
use pipedash_plugin_api::{
    PipelineRun,
    PluginError,
    PluginResult,
};

use super::{
    ProwPlugin,
    CONCURRENCY,
};
use crate::config::{
    self,
    Bucket,
    Scope,
    Selection,
};
use crate::mapper;
use crate::storage::{
    self,
    Artifacts,
    HistoryRoot,
    StoredBuild,
};
use crate::types::{
    run_number_of,
    ProwJob,
};

const HISTORY_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;
const HISTORY_WINDOW_GROWTH: i64 = 4;

impl ProwPlugin {
    pub(super) async fn scope_jobs(&self, scope: Option<&Scope>) -> PluginResult<Vec<ProwJob>> {
        self.deck()?
            .jobs(
                scope.map(|scope| scope.org.as_str()),
                scope.map(|scope| scope.repo.as_str()),
            )
            .await
    }

    pub(super) async fn jobs_by_scope<'a>(
        &self, wanted: impl Iterator<Item = Option<&'a Scope>>,
    ) -> PluginResult<HashMap<Option<&'a Scope>, Vec<ProwJob>>> {
        let mut scopes: Vec<Option<&Scope>> = Vec::new();
        for scope in wanted {
            if !scopes.contains(&scope) {
                scopes.push(scope);
            }
        }
        let fetches: Vec<_> = scopes.iter().map(|scope| self.scope_jobs(*scope)).collect();
        let lists: Vec<Vec<ProwJob>> = try_buffered(fetches, CONCURRENCY).await?;
        Ok(scopes.into_iter().zip(lists).collect())
    }

    fn history_location(&self, newest: Option<&ProwJob>) -> Option<(Bucket, Vec<HistoryRoot>)> {
        if let Some(job) = newest {
            if let Some(artifacts) = Artifacts::from_view_url(&job.status.url) {
                return Some((
                    Bucket {
                        provider: artifacts.provider,
                        name: artifacts.bucket,
                    },
                    vec![HistoryRoot::for_job_type(&job.spec.job_type)],
                ));
            }
        }
        self.storage_bucket
            .clone()
            .map(|bucket| (bucket, vec![HistoryRoot::Logs, HistoryRoot::PullDirectory]))
    }

    pub(super) async fn stored_latest(&self, job: &str) -> PluginResult<Option<StoredBuild>> {
        let Some((bucket, roots)) = self.history_location(None) else {
            return Ok(None);
        };
        let storage = self.storage()?;
        for root in roots {
            if let Some(build) = storage.latest_build(&bucket, root, job).await? {
                return Ok(Some(build));
            }
        }
        Ok(None)
    }

    pub(super) fn find_build<'a>(
        jobs: &'a [ProwJob], job: &str, run_number: i64,
    ) -> Option<&'a ProwJob> {
        jobs.iter().find(|candidate| {
            candidate.spec.job == job && candidate.run_number() == Some(run_number)
        })
    }

    pub(super) async fn history_from(
        &self, jobs: &[ProwJob], selection: &Selection, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let deck = self.deck()?;
        let mut runs = mapper::runs(jobs, &selection.job, pipeline_id, deck.base());
        if self.history == config::HistorySource::Storage {
            if let Some((bucket, roots)) =
                self.history_location(mapper::newest(jobs, &selection.job))
            {
                let stored = self
                    .stored_runs(&bucket, &roots, selection, pipeline_id, &runs, limit)
                    .await?;
                runs.extend(stored);
            }
        }
        runs.sort_by_key(|run| std::cmp::Reverse(run.run_number));
        runs.truncate(limit);
        Ok(runs)
    }

    async fn stored_runs(
        &self, bucket: &Bucket, roots: &[HistoryRoot], selection: &Selection, pipeline_id: &str,
        deck_runs: &[PipelineRun], limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let storage = self.storage()?;
        let deck = self.deck()?;
        let floor = deck_runs
            .get(limit.saturating_sub(1))
            .filter(|_| limit > 0)
            .map_or(i64::MIN, |run| run.run_number);
        let now = Utc::now().timestamp_millis();

        for root in roots {
            let mut seen: HashSet<i64> = deck_runs.iter().map(|run| run.run_number).collect();
            let mut stored: Vec<PipelineRun> = Vec::new();
            let mut listed_any = false;
            let mut span = HISTORY_WINDOW_MS;
            loop {
                let from = storage::listing_floor(now.saturating_sub(span));
                let numbers = storage
                    .build_numbers(bucket, *root, &selection.job, from, None)
                    .await?;
                listed_any |= !numbers.is_empty();
                let candidates: Vec<i64> = numbers
                    .into_iter()
                    .filter(|build_id| {
                        let run_number = run_number_of(*build_id);
                        run_number > floor && seen.insert(run_number)
                    })
                    .collect();
                for chunk in candidates.chunks(limit.max(1)) {
                    let builds = storage
                        .builds(bucket, *root, &selection.job, chunk.to_vec())
                        .await?;
                    stored.extend(builds.iter().map(|build| {
                        mapper::map_stored_run(
                            build,
                            pipeline_id,
                            selection.scope.as_ref(),
                            deck.base(),
                        )
                    }));
                    if stored.len() >= limit {
                        return Ok(stored);
                    }
                }
                if from.is_none_or(|from| run_number_of(from) <= floor) {
                    break;
                }
                span = span.saturating_mul(HISTORY_WINDOW_GROWTH);
            }
            if listed_any {
                return Ok(stored);
            }
        }
        Ok(Vec::new())
    }

    pub(super) async fn fetch_run_history_impl(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let selection = config::parse_pipeline_id(pipeline_id)?;
        let jobs = self.scope_jobs(selection.scope.as_ref()).await?;
        self.history_from(&jobs, &selection, pipeline_id, limit)
            .await
    }

    pub(super) async fn fetch_run_details_impl(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let selection = config::parse_pipeline_id(pipeline_id)?;
        let deck = self.deck()?;
        let jobs = self.scope_jobs(selection.scope.as_ref()).await?;

        if let Some(job) = Self::find_build(&jobs, &selection.job, run_number) {
            return mapper::map_run(job, pipeline_id, deck.base()).ok_or_else(|| {
                PluginError::PipelineNotFound(format!(
                    "Build {} of {} has not started yet",
                    job.status.build_id, selection.job
                ))
            });
        }

        if self.history == config::HistorySource::Storage {
            if let Some((bucket, roots)) =
                self.history_location(mapper::newest(&jobs, &selection.job))
            {
                let storage = self.storage()?;
                for root in roots {
                    if let Some(build) = storage
                        .find_build(&bucket, root, &selection.job, run_number)
                        .await?
                    {
                        return Ok(mapper::map_stored_run(
                            &build,
                            pipeline_id,
                            selection.scope.as_ref(),
                            deck.base(),
                        ));
                    }
                }
            }
        }

        Err(PluginError::PipelineNotFound(format!(
            "This build of {} is no longer listed by Deck",
            selection.job
        )))
    }
}
