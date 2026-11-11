use std::collections::HashMap;

use super::PipelineService;
use crate::application::services::provider_service::ProviderLease;
use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
    PipelineRun,
    Provider,
};
use crate::event::CoreEvent;
use crate::infrastructure::deduplication::{
    hash_pipeline_run,
    hash_request,
};

mod filter;
mod query;
#[cfg(test)]
mod tests;

impl PipelineService {
    pub async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> DomainResult<Vec<PipelineRun>> {
        let pipeline = self.repository.get_cached_pipeline(pipeline_id).await?;
        let (mut runs, _) = self.fetch_run_history_leased(&pipeline, limit).await?;
        self.presenter.runs(&pipeline, &mut runs).await?;
        Ok(runs)
    }

    pub(super) async fn fetch_run_history_leased(
        &self, pipeline: &Pipeline, limit: usize,
    ) -> DomainResult<(Vec<PipelineRun>, ProviderLease)> {
        let pipeline_id = pipeline.id.as_str();
        let lease = self.provider_service.acquire(pipeline.provider_id).await?;
        let request_id = hash_request(
            pipeline.provider_id,
            lease.generation,
            &format!("fetch_runs_{}_{}", pipeline_id, limit),
        );

        let fetch_lease = lease.clone();
        let pipeline_id_owned = pipeline_id.to_string();
        let api_runs = self
            .run_deduplicator
            .deduplicate(request_id, || async move {
                fetch_lease
                    .provider
                    .fetch_run_history(&pipeline_id_owned, limit)
                    .await
            })
            .await?;

        self.provider_service
            .commit(&lease, || {
                self.merge_run_cache(lease.provider.as_ref(), pipeline, &api_runs)
            })
            .await
            .transpose()?;

        Ok((api_runs, lease))
    }

    pub(super) async fn merge_run_cache(
        &self, provider: &dyn Provider, pipeline: &Pipeline, api_runs: &[PipelineRun],
    ) -> DomainResult<()> {
        let pipeline_id = pipeline.id.as_str();
        let cached_with_hashes = self
            .repository
            .get_cached_runs_with_hashes(pipeline_id)
            .await?;

        let mut api_map: HashMap<i64, (PipelineRun, String)> = HashMap::new();
        for run in api_runs {
            api_map.insert(run.run_number, (run.clone(), hash_pipeline_run(run)));
        }

        let mut new_runs = Vec::new();
        let mut changed_runs = Vec::new();
        let mut deleted_run_numbers = Vec::new();

        for (run_number, (run, api_hash)) in &api_map {
            if let Some((cached_run, cached_hash)) = cached_with_hashes.get(run_number) {
                if api_hash != cached_hash || cached_run.metadata != run.metadata {
                    changed_runs.push(run.clone());
                }
            } else {
                new_runs.push(run.clone());
            }
        }

        let min_api_run_number = api_runs
            .iter()
            .map(|r| r.run_number)
            .min()
            .unwrap_or(i64::MAX);
        let policy = provider.run_history_policy();
        for (run_number, (cached_run, _)) in &cached_with_hashes {
            let snapshot = cached_run
                .metadata
                .get("snapshot")
                .and_then(serde_json::Value::as_bool)
                == Some(true);
            if api_map.contains_key(run_number) {
                continue;
            }
            if policy.bounded && provider.can_archive_run(pipeline, cached_run) {
                if cached_run.metadata.get("archived") != Some(&serde_json::Value::Bool(true)) {
                    changed_runs.extend(archived_run(provider, pipeline, cached_run.clone()));
                }
            } else if *run_number >= min_api_run_number || snapshot || policy.bounded {
                deleted_run_numbers.push(*run_number);
            }
        }

        if !new_runs.is_empty() || !changed_runs.is_empty() || !deleted_run_numbers.is_empty() {
            self.repository
                .merge_run_cache(pipeline_id, new_runs, changed_runs, deleted_run_numbers)
                .await?;

            self.event_bus
                .emit(CoreEvent::RunHistoryCacheInvalidated {
                    pipeline_id: Some(pipeline_id.to_string()),
                    run_number: None,
                })
                .await;
        }

        Ok(())
    }

    pub async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<PipelineRun> {
        let pipeline = self.repository.get_cached_pipeline(pipeline_id).await?;

        let provider = self
            .provider_service
            .get_provider(pipeline.provider_id)
            .await?;
        let mut run = self
            .fetch_run_details_from(provider.as_ref(), &pipeline, run_number)
            .await?;
        self.presenter
            .runs(&pipeline, std::slice::from_mut(&mut run))
            .await?;
        Ok(run)
    }

    pub(super) async fn fetch_run_details_from(
        &self, provider: &dyn Provider, pipeline: &Pipeline, run_number: i64,
    ) -> DomainResult<PipelineRun> {
        let pipeline_id = pipeline.id.as_str();
        match provider.fetch_run_details(pipeline_id, run_number).await {
            Err(error @ DomainError::PipelineNotFound(_))
                if provider.run_history_policy().bounded =>
            {
                let cached = self
                    .repository
                    .get_cached_run(pipeline_id, run_number)
                    .await?;
                cached
                    .and_then(|run| archived_run(provider, pipeline, run))
                    .ok_or(error)
            }
            result => result,
        }
    }
}

pub(super) fn archived_run(
    provider: &dyn Provider, pipeline: &Pipeline, mut run: PipelineRun,
) -> Option<PipelineRun> {
    if !provider.can_archive_run(pipeline, &run) {
        return None;
    }
    run.metadata
        .insert("archived".to_string(), serde_json::Value::Bool(true));
    Some(run)
}
