use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashSet;
use tokio::sync::Semaphore;
use tokio::time::timeout;

const MAX_CONCURRENT_PROVIDER_FETCHES: usize = 10;

use super::metrics_service::MetricsService;
use super::provider_service::ProviderService;
use crate::domain::{
    DomainError,
    DomainResult,
    PaginatedRunHistory,
    Pipeline,
    PipelineRun,
    TriggerParams,
};
use crate::event::{
    CacheInvalidationReason,
    CoreEvent,
    EventBus,
};
use crate::infrastructure::database::Repository;
use crate::infrastructure::deduplication::{
    hash_pipeline_run,
    hash_request,
    RequestDeduplicator,
};

pub struct PipelineService {
    repository: Arc<Repository>,
    provider_service: Arc<ProviderService>,
    metrics_service: Option<Arc<MetricsService>>,
    event_bus: Arc<dyn EventBus>,
    deduplicator: Arc<RequestDeduplicator<Vec<Pipeline>>>,
    run_deduplicator: Arc<RequestDeduplicator<Vec<PipelineRun>>>,
    cache_write_tracker: Arc<DashSet<String>>,
}

impl PipelineService {
    pub fn new(
        repository: Arc<Repository>, provider_service: Arc<ProviderService>,
        metrics_service: Option<Arc<MetricsService>>, event_bus: Arc<dyn EventBus>,
    ) -> Self {
        Self {
            repository,
            provider_service,
            metrics_service,
            event_bus,
            deduplicator: Arc::new(RequestDeduplicator::new()),
            run_deduplicator: Arc::new(RequestDeduplicator::new()),
            cache_write_tracker: Arc::new(DashSet::new()),
        }
    }

    async fn update_provider_status_and_emit(
        &self, provider_id: i64, success: bool, error: Option<String>,
    ) {
        if let Ok(changed) = self
            .repository
            .update_provider_fetch_status(provider_id, success, error)
            .await
        {
            if changed {
                self.event_bus.emit(CoreEvent::ProvidersChanged).await;
            }
        }
    }

    pub async fn fetch_pipelines(&self, provider_id: Option<i64>) -> DomainResult<Vec<Pipeline>> {
        if let Err(_e) = self.repository.clear_workflow_parameters_cache().await {}

        if let Some(pid) = provider_id {
            let provider = self.provider_service.get_provider(pid).await?;

            let result = timeout(Duration::from_secs(30), provider.fetch_pipelines()).await;

            match result {
                Ok(Ok(pipelines)) => {
                    self.repository
                        .update_pipelines_cache(pid, &pipelines)
                        .await?;

                    self.update_provider_status_and_emit(pid, true, None).await;

                    self.event_bus
                        .emit(CoreEvent::PipelineCacheInvalidated {
                            provider_id: Some(pid),
                            reason: CacheInvalidationReason::Fetch,
                        })
                        .await;

                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as i64;

                    self.event_bus
                        .emit(CoreEvent::PipelinesUpdated {
                            pipelines: pipelines.clone(),
                            provider_id: Some(pid),
                            timestamp,
                        })
                        .await;

                    Ok(pipelines)
                }
                Ok(Err(e)) => {
                    let error_msg = format!("{e}");
                    self.update_provider_status_and_emit(pid, false, Some(error_msg))
                        .await;

                    Err(e)
                }
                Err(_elapsed) => {
                    let error_msg = "Connection timeout - provider did not respond".to_string();
                    self.update_provider_status_and_emit(pid, false, Some(error_msg.clone()))
                        .await;

                    Err(DomainError::ProviderError(error_msg))
                }
            }
        } else {
            let provider_summaries = self.provider_service.list_providers().await?;

            let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_PROVIDER_FETCHES));

            let futures: Vec<_> = provider_summaries
                .into_iter()
                .map(|summary| {
                    let semaphore = semaphore.clone();
                    let provider_service = self.provider_service.clone();
                    let deduplicator = self.deduplicator.clone();
                    let repository = self.repository.clone();
                    let event_bus = self.event_bus.clone();
                    let provider_id = summary.id;
                    async move {
                        let _permit = semaphore.acquire().await.expect("semaphore closed");
                        let request_id = hash_request(provider_id, "fetch_pipelines");

                        let result = timeout(
                            Duration::from_secs(30),
                            deduplicator.deduplicate(request_id, || async {
                                let provider = provider_service.get_provider(provider_id).await?;
                                provider.fetch_pipelines().await
                            }),
                        )
                        .await;

                        match result {
                            Ok(Ok(pipelines)) => {
                                if let Ok(changed) = repository
                                    .update_provider_fetch_status(provider_id, true, None)
                                    .await
                                {
                                    if changed {
                                        event_bus.emit(CoreEvent::ProvidersChanged).await;
                                    }
                                }

                                Ok((provider_id, pipelines))
                            }
                            Ok(Err(e)) => {
                                let error_msg = format!("{e}");
                                if let Ok(changed) = repository
                                    .update_provider_fetch_status(
                                        provider_id,
                                        false,
                                        Some(error_msg),
                                    )
                                    .await
                                {
                                    if changed {
                                        event_bus.emit(CoreEvent::ProvidersChanged).await;
                                    }
                                }

                                Err(e)
                            }
                            Err(_elapsed) => {
                                let error_msg =
                                    "Connection timeout - provider did not respond".to_string();
                                if let Ok(changed) = repository
                                    .update_provider_fetch_status(
                                        provider_id,
                                        false,
                                        Some(error_msg.clone()),
                                    )
                                    .await
                                {
                                    if changed {
                                        event_bus.emit(CoreEvent::ProvidersChanged).await;
                                    }
                                }

                                Err(DomainError::ProviderError(error_msg))
                            }
                        }
                    }
                })
                .collect();

            let results =
                timeout(Duration::from_secs(60), futures::future::join_all(futures)).await;

            let results = match results {
                Ok(results) => results,
                Err(_) => {
                    tracing::warn!("Overall provider fetch timeout (60s exceeded)");
                    return Err(DomainError::ProviderError(
                        "Overall provider fetch timeout (60s exceeded)".into(),
                    ));
                }
            };

            let mut all_pipelines = Vec::new();
            for result in results {
                match result {
                    Ok((provider_id, pipelines)) => {
                        self.repository
                            .update_pipelines_cache(provider_id, &pipelines)
                            .await?;

                        self.event_bus
                            .emit(CoreEvent::PipelineCacheInvalidated {
                                provider_id: Some(provider_id),
                                reason: CacheInvalidationReason::Fetch,
                            })
                            .await;

                        all_pipelines.extend(pipelines);
                    }
                    Err(_e) => {}
                }
            }

            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;

            self.event_bus
                .emit(CoreEvent::PipelinesUpdated {
                    pipelines: all_pipelines.clone(),
                    provider_id: None,
                    timestamp,
                })
                .await;

            Ok(all_pipelines)
        }
    }

    pub async fn get_cached_pipelines(
        &self, provider_id: Option<i64>,
    ) -> DomainResult<Vec<Pipeline>> {
        self.repository.get_cached_pipelines(provider_id).await
    }

    pub async fn fetch_pipelines_lazy(
        &self, provider_id: Option<i64>, page: usize, page_size: usize,
    ) -> DomainResult<pipedash_plugin_api::PaginatedResponse<Pipeline>> {
        use pipedash_plugin_api::PaginatedResponse;

        if let Some(pid) = provider_id {
            let provider = self.provider_service.get_provider(pid).await?;
            let result = provider.fetch_pipelines_paginated(page, page_size).await?;

            if page == 1 {
                self.repository
                    .update_pipelines_cache(pid, &result.items)
                    .await?;
            }

            Ok(result)
        } else {
            let all_pipelines = self.fetch_pipelines(None).await?;
            let total_count = all_pipelines.len();
            let start = (page - 1) * page_size;
            let end = start + page_size;

            let pipelines = if start < total_count {
                all_pipelines[start..end.min(total_count)].to_vec()
            } else {
                Vec::new()
            };

            Ok(PaginatedResponse::new(
                pipelines,
                page,
                page_size,
                total_count,
            ))
        }
    }

    pub async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> DomainResult<Vec<PipelineRun>> {
        let cached_pipelines = self.repository.get_cached_pipelines(None).await?;
        let pipeline = cached_pipelines
            .iter()
            .find(|p| p.id == pipeline_id)
            .ok_or_else(|| DomainError::PipelineNotFound(pipeline_id.to_string()))?;

        let provider_id = pipeline.provider_id;
        let pipeline_id_owned = pipeline_id.to_string();

        let request_id = hash_request(
            provider_id,
            &format!("fetch_runs_{}_{}", pipeline_id, limit),
        );

        let provider_service = self.provider_service.clone();
        let run_deduplicator = self.run_deduplicator.clone();

        let api_runs = run_deduplicator
            .deduplicate(request_id, || async move {
                let provider = provider_service.get_provider(provider_id).await?;
                provider.fetch_run_history(&pipeline_id_owned, limit).await
            })
            .await?;

        let cached_with_hashes = self
            .repository
            .get_cached_runs_with_hashes(pipeline_id)
            .await?;

        let mut api_map: HashMap<i64, (PipelineRun, String)> = HashMap::new();
        for run in &api_runs {
            let status_str = run.status.as_str();
            let hash = hash_pipeline_run(
                run.run_number,
                status_str,
                run.branch.as_deref(),
                &run.started_at.to_rfc3339(),
                run.duration_seconds,
                run.commit_sha.as_deref(),
            );
            api_map.insert(run.run_number, (run.clone(), hash));
        }

        let mut new_runs = Vec::new();
        let mut changed_runs = Vec::new();
        let mut deleted_run_numbers = Vec::new();

        for (run_number, (run, api_hash)) in &api_map {
            if let Some((_, cached_hash)) = cached_with_hashes.get(run_number) {
                if api_hash != cached_hash {
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
        for run_number in cached_with_hashes.keys() {
            if *run_number >= min_api_run_number && !api_map.contains_key(run_number) {
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
                })
                .await;
        }

        Ok(api_runs)
    }

    pub async fn fetch_run_history_paginated(
        &self, pipeline_id: &str, page: usize, page_size: usize,
    ) -> DomainResult<PaginatedRunHistory> {
        let start_idx = (page - 1) * page_size;
        let end_idx = start_idx + page_size;

        let cached_count = self.repository.get_cached_run_count(pipeline_id).await?;

        if cached_count >= end_idx {
            let runs = self
                .repository
                .get_paginated_runs(pipeline_id, page, page_size)
                .await?;

            let is_complete = false;
            let total_pages = cached_count.div_ceil(page_size);

            if page == 1 {
                if let Some(metrics_service) = &self.metrics_service {
                    let pipeline_id_clone = pipeline_id.to_string();
                    let metrics_service_clone = metrics_service.clone();
                    let event_bus = self.event_bus.clone();
                    let repository = self.repository.clone();

                    tokio::spawn(async move {
                        match repository
                            .get_cached_run_history(&pipeline_id_clone, 10000)
                            .await
                        {
                            Ok(all_cached_runs) => {
                                match metrics_service_clone
                                    .extract_and_store_metrics(&pipeline_id_clone, &all_cached_runs)
                                    .await
                                {
                                    Ok(count) => {
                                        if count > 0 {
                                            event_bus
                                                .emit(CoreEvent::MetricsGenerated {
                                                    pipeline_id: pipeline_id_clone,
                                                })
                                                .await;
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            error = %e,
                                            pipeline_id = %pipeline_id_clone,
                                            "Failed to extract metrics from cached runs"
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::warn!(
                                    error = %e,
                                    pipeline_id = %pipeline_id_clone,
                                    "Failed to load cached runs for metrics extraction"
                                );
                            }
                        }
                    });
                }
            } // end if page == 1

            return Ok(PaginatedRunHistory {
                runs,
                total_count: cached_count,
                has_more: !is_complete,
                is_complete,
                page,
                page_size,
                total_pages,
            });
        }

        let needed_runs = page * page_size;

        let github_pages_needed = (needed_runs / 100) + 1;
        let fetch_limit = (github_pages_needed * 100).min(1000);

        tracing::debug!(
            page = page,
            page_size = page_size,
            needed_runs = needed_runs,
            fetch_limit = fetch_limit,
            "Smart pagination: fetching only what's needed"
        );

        let all_runs = self.fetch_run_history(pipeline_id, fetch_limit).await?;
        let total_count = all_runs.len();

        let is_complete = total_count < fetch_limit;

        let repository = self.repository.clone();
        let pipeline_id_for_cache = pipeline_id.to_string();
        let runs_to_cache = all_runs.clone();
        let cache_tracker = self.cache_write_tracker.clone();
        tokio::spawn(async move {
            if !cache_tracker.insert(pipeline_id_for_cache.clone()) {
                return;
            }
            if let Err(e) = repository
                .cache_run_history(&pipeline_id_for_cache, &runs_to_cache)
                .await
            {
                tracing::warn!(error = %e, pipeline_id = %pipeline_id_for_cache, "Failed to cache run history");
            }
            cache_tracker.remove(&pipeline_id_for_cache);
        });

        if let Some(metrics_service) = &self.metrics_service {
            let pipeline_id_clone = pipeline_id.to_string();
            let runs_clone = all_runs.clone();
            let metrics_service_clone = metrics_service.clone();
            let event_bus = self.event_bus.clone();

            tokio::spawn(async move {
                match metrics_service_clone
                    .extract_and_store_metrics(&pipeline_id_clone, &runs_clone)
                    .await
                {
                    Ok(count) => {
                        if count > 0 {
                            event_bus
                                .emit(CoreEvent::MetricsGenerated {
                                    pipeline_id: pipeline_id_clone,
                                })
                                .await;
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            error = %e,
                            pipeline_id = %pipeline_id_clone,
                            "Failed to extract and store metrics"
                        );
                    }
                }
            });
        }

        let runs = if start_idx < total_count {
            let end = end_idx.min(total_count);
            all_runs[start_idx..end].to_vec()
        } else {
            Vec::new()
        };

        let total_pages = total_count.div_ceil(page_size);

        Ok(PaginatedRunHistory {
            runs,
            total_count,
            has_more: !is_complete,
            is_complete,
            page,
            page_size,
            total_pages,
        })
    }

    pub async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<PipelineRun> {
        let cached_pipelines = self.repository.get_cached_pipelines(None).await?;
        let pipeline = cached_pipelines
            .iter()
            .find(|p| p.id == pipeline_id)
            .ok_or_else(|| DomainError::PipelineNotFound(pipeline_id.to_string()))?;

        let provider = self
            .provider_service
            .get_provider(pipeline.provider_id)
            .await?;
        provider.fetch_run_details(pipeline_id, run_number).await
    }

    pub async fn trigger_pipeline(&self, params: TriggerParams) -> DomainResult<String> {
        let cached_pipelines = self.repository.get_cached_pipelines(None).await?;
        let pipeline = cached_pipelines
            .iter()
            .find(|p| p.id == params.workflow_id)
            .ok_or_else(|| DomainError::PipelineNotFound(params.workflow_id.clone()))?;

        let provider = self
            .provider_service
            .get_provider(pipeline.provider_id)
            .await?;
        let result = provider.trigger_pipeline(params.clone()).await?;

        self.event_bus
            .emit(CoreEvent::RunTriggered {
                workflow_id: params.workflow_id,
            })
            .await;

        Ok(result)
    }

    pub async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()> {
        let cached_pipelines = self.repository.get_cached_pipelines(None).await?;
        let pipeline = cached_pipelines
            .iter()
            .find(|p| p.id == pipeline_id)
            .ok_or_else(|| DomainError::PipelineNotFound(pipeline_id.to_string()))?;

        let provider = self
            .provider_service
            .get_provider(pipeline.provider_id)
            .await?;
        provider.cancel_run(pipeline_id, run_number).await?;

        self.event_bus
            .emit(CoreEvent::RunCancelled {
                pipeline_id: pipeline_id.to_string(),
            })
            .await;

        Ok(())
    }

    pub async fn refresh_all(&self) -> DomainResult<()> {
        self.fetch_pipelines(None).await?;
        Ok(())
    }

    pub async fn clear_run_history_cache(&self, pipeline_id: &str) {
        let _ = self.repository.clear_cached_run_history(pipeline_id).await;
    }

    pub async fn clear_all_run_history_caches(&self) {
        let _ = self.repository.clear_all_run_history_cache().await;
    }

    pub async fn get_pipelines_cache_count(&self) -> DomainResult<i64> {
        self.repository.get_pipelines_cache_count().await
    }

    pub async fn get_run_history_cache_count(&self) -> DomainResult<i64> {
        self.repository.get_run_history_cache_count().await
    }

    pub async fn get_workflow_params_cache_count(&self) -> DomainResult<i64> {
        self.repository.get_workflow_params_cache_count().await
    }

    pub async fn clear_pipelines_cache(&self) -> DomainResult<usize> {
        self.repository.clear_pipelines_cache().await
    }

    pub async fn clear_workflow_params_cache(&self) -> DomainResult<()> {
        self.repository.clear_workflow_parameters_cache().await
    }

    pub async fn clear_all_caches_atomic(&self) -> DomainResult<()> {
        self.repository.clear_all_caches_atomic().await
    }

    pub async fn invalidate_run_cache(&self, pipeline_id: &str) {
        let _ = self.repository.clear_cached_run_history(pipeline_id).await;
    }

    pub async fn get_cached_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> DomainResult<Vec<PipelineRun>> {
        self.repository
            .get_cached_run_history(pipeline_id, limit)
            .await
    }
}
