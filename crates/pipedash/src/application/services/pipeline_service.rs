use std::collections::HashMap;
use std::sync::Arc;

use tauri::Emitter;

use crate::application::services::{
    MetricsService,
    ProviderService,
};
use crate::domain::{
    DomainResult,
    PaginatedRunHistory,
    Pipeline,
    PipelineRun,
    TriggerParams,
};
use crate::infrastructure::database::Repository;
use crate::infrastructure::deduplication::{
    hash_request,
    RequestDeduplicator,
};

pub struct PipelineService {
    repository: Arc<Repository>,
    provider_service: Arc<ProviderService>,
    metrics_service: Option<Arc<MetricsService>>,
    deduplicator: Arc<RequestDeduplicator<Vec<Pipeline>>>,
    run_deduplicator: Arc<RequestDeduplicator<Vec<PipelineRun>>>,
}

impl PipelineService {
    pub fn new(
        repository: Arc<Repository>, provider_service: Arc<ProviderService>,
        metrics_service: Option<Arc<MetricsService>>,
    ) -> Self {
        Self {
            repository,
            provider_service,
            metrics_service,
            deduplicator: Arc::new(RequestDeduplicator::new()),
            run_deduplicator: Arc::new(RequestDeduplicator::new()),
        }
    }

    pub async fn fetch_pipelines(&self, provider_id: Option<i64>) -> DomainResult<Vec<Pipeline>> {
        let start = std::time::Instant::now();

        if let Err(e) = self.repository.clear_workflow_parameters_cache().await {
            eprintln!("[WARN] Failed to clear workflow parameters cache: {e}");
        }

        if let Some(pid) = provider_id {
            eprintln!("[PERF] Fetching pipelines for single provider {pid}");
            let provider_start = std::time::Instant::now();
            let provider = self.provider_service.get_provider(pid).await?;
            eprintln!("[PERF] Got provider in {:?}", provider_start.elapsed());

            let fetch_start = std::time::Instant::now();
            let pipelines = provider.fetch_pipelines().await?;
            eprintln!(
                "[PERF] Fetched {} pipelines in {:?}",
                pipelines.len(),
                fetch_start.elapsed()
            );

            let cache_start = std::time::Instant::now();
            self.repository
                .update_pipelines_cache(pid, &pipelines)
                .await?;
            eprintln!("[PERF] Updated cache in {:?}", cache_start.elapsed());

            eprintln!("[PERF] Total fetch_pipelines time: {:?}", start.elapsed());
            Ok(pipelines)
        } else {
            eprintln!("[PERF] Fetching pipelines for all providers");
            let list_start = std::time::Instant::now();
            let provider_summaries = self.provider_service.list_providers().await?;
            eprintln!(
                "[PERF] Listed {} providers in {:?}",
                provider_summaries.len(),
                list_start.elapsed()
            );

            let parallel_start = std::time::Instant::now();
            let futures: Vec<_> = provider_summaries
                .into_iter()
                .map(|summary| {
                    let provider_service = self.provider_service.clone();
                    let deduplicator = self.deduplicator.clone();
                    let provider_id = summary.id;
                    let provider_name = summary.name.clone();
                    async move {
                        let request_id = hash_request(provider_id, "fetch_pipelines");
                        let fetch_start = std::time::Instant::now();

                        let pipelines = deduplicator
                            .deduplicate(request_id, || async {
                                let provider = provider_service.get_provider(provider_id).await?;
                                provider.fetch_pipelines().await
                            })
                            .await?;

                        eprintln!(
                            "[PERF] Provider '{}' fetched {} pipelines in {:?}",
                            provider_name,
                            pipelines.len(),
                            fetch_start.elapsed()
                        );
                        Ok::<(i64, Vec<Pipeline>), crate::domain::DomainError>((
                            provider_id,
                            pipelines,
                        ))
                    }
                })
                .collect();

            let results = futures::future::join_all(futures).await;
            eprintln!(
                "[PERF] All providers fetched in parallel in {:?}",
                parallel_start.elapsed()
            );

            let cache_start = std::time::Instant::now();
            let mut all_pipelines = Vec::new();
            for result in results {
                match result {
                    Ok((provider_id, pipelines)) => {
                        self.repository
                            .update_pipelines_cache(provider_id, &pipelines)
                            .await?;
                        all_pipelines.extend(pipelines);
                    }
                    Err(e) => {
                        eprintln!("[ERROR] Failed to fetch pipelines from provider: {e:?}");
                    }
                }
            }
            eprintln!(
                "[PERF] Updated cache for {} pipelines in {:?}",
                all_pipelines.len(),
                cache_start.elapsed()
            );

            eprintln!("[PERF] Total fetch_pipelines time: {:?}", start.elapsed());
            Ok(all_pipelines)
        }
    }

    pub async fn get_cached_pipelines(
        &self, provider_id: Option<i64>,
    ) -> DomainResult<Vec<Pipeline>> {
        self.repository.get_cached_pipelines(provider_id).await
    }

    pub async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> DomainResult<Vec<PipelineRun>> {
        let cached_pipelines = self.repository.get_cached_pipelines(None).await?;
        let pipeline = cached_pipelines
            .iter()
            .find(|p| p.id == pipeline_id)
            .ok_or_else(|| crate::domain::DomainError::PipelineNotFound(pipeline_id.to_string()))?;

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
            let hash = crate::infrastructure::deduplication::hash_pipeline_run(
                run.run_number,
                status_str,
                &run.branch,
                &run.started_at.to_rfc3339(),
                run.duration_seconds,
                &run.commit_sha,
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
        } else {
            eprintln!(
                "[CACHE] No incremental changes detected for {}",
                pipeline_id
            );
        }

        Ok(api_runs)
    }

    pub async fn fetch_run_history_paginated(
        &self, pipeline_id: &str, page: usize, page_size: usize,
        app_handle: Option<tauri::AppHandle>,
    ) -> DomainResult<PaginatedRunHistory> {
        let start = std::time::Instant::now();
        let start_idx = (page - 1) * page_size;
        let end_idx = start_idx + page_size;

        let db_cached_all = self
            .repository
            .get_cached_run_history(pipeline_id, 10000)
            .await?;

        if !db_cached_all.is_empty() {
            eprintln!("[DB CACHE HIT] Loaded {} runs from DB", db_cached_all.len());

            let total_count = db_cached_all.len();
            let runs = if start_idx < total_count {
                let end = end_idx.min(total_count);
                db_cached_all[start_idx..end].to_vec()
            } else {
                Vec::new()
            };

            let is_complete = total_count < 1000;
            let total_pages = if is_complete {
                total_count.div_ceil(page_size)
            } else {
                // Allow fetching next page to get more
                total_count.div_ceil(page_size) + 1
            };

            eprintln!(
                "[PERF] Returned {} runs from cache in {:?}",
                runs.len(),
                start.elapsed()
            );

            return Ok(PaginatedRunHistory {
                runs,
                total_count,
                has_more: !is_complete,
                is_complete,
                page,
                page_size,
                total_pages,
            });
        }
        let fetch_limit = end_idx.max(100);
        let fetch_limit = fetch_limit.min(1000);

        eprintln!(
            "[CACHE MISS] Page {}: fetching {} runs from API",
            page, fetch_limit
        );

        let all_runs = self.fetch_run_history(pipeline_id, fetch_limit).await?;
        let total_count = all_runs.len();
        let is_complete = total_count < fetch_limit;

        let repository = self.repository.clone();
        let pipeline_id_owned = pipeline_id.to_string();
        let runs_to_cache = all_runs.clone();
        tokio::spawn(async move {
            if let Err(e) = repository
                .cache_run_history(&pipeline_id_owned, &runs_to_cache)
                .await
            {
                eprintln!("[WARN] Failed to persist run history: {e:?}");
            }
        });

        if let Some(metrics_service) = &self.metrics_service {
            let pipeline_id_clone = pipeline_id.to_string();
            let runs_clone = all_runs.clone();
            let metrics_service_clone = metrics_service.clone();
            let app_handle_clone = app_handle.clone();

            tokio::spawn(async move {
                eprintln!(
                    "[METRICS] Background task started for {}",
                    pipeline_id_clone
                );

                match metrics_service_clone
                    .extract_and_store_metrics(&pipeline_id_clone, &runs_clone)
                    .await
                {
                    Ok(count) if count > 0 => {
                        eprintln!(
                            "[METRICS] ✓ Stored {} metrics for {}",
                            count, pipeline_id_clone
                        );

                        if let Some(handle) = app_handle_clone {
                            let _ = handle.emit("metrics-generated", &pipeline_id_clone);
                        }
                    }
                    Ok(_) => {
                        eprintln!(
                            "[METRICS] ✓ No new metrics for {} (disabled or already processed)",
                            pipeline_id_clone
                        );
                    }
                    Err(e) => {
                        eprintln!("[METRICS] ✗ Failed for {}: {:?}", pipeline_id_clone, e);
                    }
                }

                eprintln!(
                    "[METRICS] Background task completed for {}",
                    pipeline_id_clone
                );
            });
        }

        // Slice to requested page
        let runs = if start_idx < total_count {
            let end = end_idx.min(total_count);
            all_runs[start_idx..end].to_vec()
        } else {
            Vec::new()
        };

        let total_pages = if is_complete {
            total_count.div_ceil(page_size)
        } else {
            total_count.div_ceil(page_size) + 1
        };

        eprintln!(
            "[PERF] Returned {} runs (of {} total, complete: {}, page {} of {})",
            runs.len(),
            total_count,
            is_complete,
            page,
            total_pages
        );
        eprintln!("[PERF] Fetch completed in {:?}", start.elapsed());

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
        // Find the pipeline in cache to get the provider_id
        let cached_pipelines = self.repository.get_cached_pipelines(None).await?;
        let pipeline = cached_pipelines
            .iter()
            .find(|p| p.id == pipeline_id)
            .ok_or_else(|| crate::domain::DomainError::PipelineNotFound(pipeline_id.to_string()))?;

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
            .ok_or_else(|| {
                crate::domain::DomainError::PipelineNotFound(params.workflow_id.clone())
            })?;

        let provider = self
            .provider_service
            .get_provider(pipeline.provider_id)
            .await?;
        provider.trigger_pipeline(params).await
    }

    pub async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> DomainResult<()> {
        // Find the pipeline in cache to get the provider_id
        let cached_pipelines = self.repository.get_cached_pipelines(None).await?;
        let pipeline = cached_pipelines
            .iter()
            .find(|p| p.id == pipeline_id)
            .ok_or_else(|| crate::domain::DomainError::PipelineNotFound(pipeline_id.to_string()))?;

        let provider = self
            .provider_service
            .get_provider(pipeline.provider_id)
            .await?;
        provider.cancel_run(pipeline_id, run_number).await
    }

    pub async fn refresh_all(&self) -> DomainResult<()> {
        self.fetch_pipelines(None).await?;
        Ok(())
    }

    /// Clear run history cache for a specific pipeline (used on manual refresh)
    pub async fn clear_run_history_cache(&self, pipeline_id: &str) {
        if let Err(e) = self.repository.clear_cached_run_history(pipeline_id).await {
            eprintln!(
                "[CACHE] Failed to clear DB cache for {}: {:?}",
                pipeline_id, e
            );
        } else {
            eprintln!("[CACHE] Cleared cache for pipeline: {}", pipeline_id);
        }
    }

    pub async fn clear_all_run_history_caches(&self) {
        // Clear all run history from DB
        match self.repository.get_run_history_cache_count().await {
            Ok(count) => {
                // Delete all run_history_cache entries
                if let Err(e) = self.repository.clear_all_run_history_cache().await {
                    eprintln!("[CACHE] Failed to clear all run history caches: {:?}", e);
                } else {
                    eprintln!("[CACHE] Cleared {} cached run histories", count);
                }
            }
            Err(e) => {
                eprintln!("[CACHE] Failed to get run history count: {:?}", e);
            }
        }
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

    pub async fn invalidate_run_cache(&self, pipeline_id: &str) {
        if let Err(e) = self.repository.clear_cached_run_history(pipeline_id).await {
            eprintln!(
                "[CACHE] Failed to clear DB cache for {}: {:?}",
                pipeline_id, e
            );
        } else {
            eprintln!("[CACHE] Invalidated run history (DB) for {}", pipeline_id);
        }
    }
}
