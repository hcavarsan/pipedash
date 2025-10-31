use std::collections::HashMap;
use std::sync::Arc;
use std::time::{
    Duration,
    Instant,
};

use tokio::sync::RwLock;

use crate::application::services::ProviderService;
use crate::domain::{
    DomainResult,
    PaginatedRunHistory,
    Pipeline,
    PipelineRun,
    TriggerParams,
};
use crate::infrastructure::database::Repository;

#[derive(Clone)]
struct CachedRunHistory {
    runs: Vec<PipelineRun>,
    fetched_at: Instant,
    is_complete: bool,
}

impl CachedRunHistory {
    fn is_fresh(&self, ttl: Duration) -> bool {
        self.fetched_at.elapsed() < ttl
    }
}

pub struct PipelineService {
    repository: Arc<Repository>,
    provider_service: Arc<ProviderService>,
    run_history_cache: Arc<RwLock<HashMap<String, CachedRunHistory>>>,
    cache_ttl: Duration,
}

impl PipelineService {
    pub fn new(repository: Arc<Repository>, provider_service: Arc<ProviderService>) -> Self {
        Self {
            repository,
            provider_service,
            run_history_cache: Arc::new(RwLock::new(HashMap::new())),
            cache_ttl: Duration::from_secs(120), // 2 minutes
        }
    }

    pub async fn fetch_pipelines(&self, provider_id: Option<i64>) -> DomainResult<Vec<Pipeline>> {
        let start = std::time::Instant::now();

        if let Err(e) = self.repository.clear_workflow_parameters_cache() {
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
            self.repository.cache_pipelines(pid, &pipelines)?;
            eprintln!("[PERF] Cached pipelines in {:?}", cache_start.elapsed());

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
                    let provider_id = summary.id;
                    let provider_name = summary.name.clone();
                    async move {
                        let fetch_start = std::time::Instant::now();
                        let provider = provider_service.get_provider(provider_id).await?;
                        let pipelines = provider.fetch_pipelines().await?;
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
                        self.repository.cache_pipelines(provider_id, &pipelines)?;
                        all_pipelines.extend(pipelines);
                    }
                    Err(e) => {
                        eprintln!("[ERROR] Failed to fetch pipelines from provider: {e:?}");
                    }
                }
            }
            eprintln!(
                "[PERF] Cached all {} pipelines in {:?}",
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
        self.repository.get_cached_pipelines(provider_id)
    }

    pub async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> DomainResult<Vec<PipelineRun>> {
        let cached_pipelines = self.repository.get_cached_pipelines(None)?;
        let pipeline = cached_pipelines
            .iter()
            .find(|p| p.id == pipeline_id)
            .ok_or_else(|| crate::domain::DomainError::PipelineNotFound(pipeline_id.to_string()))?;

        let provider = self
            .provider_service
            .get_provider(pipeline.provider_id)
            .await?;
        provider.fetch_run_history(pipeline_id, limit).await
    }

    pub async fn fetch_run_history_paginated(
        &self, pipeline_id: &str, page: usize, page_size: usize,
    ) -> DomainResult<PaginatedRunHistory> {
        let start = Instant::now();
        let start_idx = (page - 1) * page_size;
        let end_idx = start_idx + page_size;

        // Check cache first
        let cache = self.run_history_cache.read().await;
        if let Some(cached) = cache.get(pipeline_id) {
            if cached.is_fresh(self.cache_ttl) {
                let cached_count = cached.runs.len();

                // If cache has enough runs for this page, use it
                if start_idx < cached_count || cached.is_complete {
                    eprintln!(
                        "[CACHE HIT] Page {}: using {} cached runs (fetched {:?} ago)",
                        page,
                        cached_count,
                        cached.fetched_at.elapsed()
                    );

                    let runs = if start_idx < cached_count {
                        let end = end_idx.min(cached_count);
                        cached.runs[start_idx..end].to_vec()
                    } else {
                        Vec::new()
                    };

                    // If not complete, add phantom page to allow navigation
                    let total_pages = if cached.is_complete {
                        cached_count.div_ceil(page_size)
                    } else {
                        cached_count.div_ceil(page_size) + 1
                    };

                    return Ok(PaginatedRunHistory {
                        runs,
                        total_count: cached_count,
                        has_more: !cached.is_complete,
                        is_complete: cached.is_complete,
                        page,
                        page_size,
                        total_pages,
                    });
                }
            } else {
                eprintln!("[CACHE EXPIRED] Clearing stale cache for {}", pipeline_id);
            }
        }
        drop(cache); // Release read lock

        // Cache miss or insufficient data - fetch from provider
        // Fetch in 100-run chunks (matches provider API page size)
        let fetch_limit = end_idx.div_ceil(100) * 100;
        let fetch_limit = fetch_limit.min(1000); // Cap at 1000 total

        eprintln!(
            "[CACHE MISS] Page {}: fetching {} runs from API",
            page, fetch_limit
        );

        let all_runs = self.fetch_run_history(pipeline_id, fetch_limit).await?;
        let total_count = all_runs.len();
        let is_complete = total_count < fetch_limit; // If we got less than requested, that's all

        // If not complete, add phantom page to allow navigation
        let total_pages = if is_complete {
            total_count.div_ceil(page_size)
        } else {
            // Show at least one more page to allow fetching more
            total_count.div_ceil(page_size) + 1
        };

        // Update cache
        let cached = CachedRunHistory {
            runs: all_runs.clone(),
            fetched_at: Instant::now(),
            is_complete,
        };

        let mut cache = self.run_history_cache.write().await;
        cache.insert(pipeline_id.to_string(), cached);
        drop(cache); // Release write lock

        // Slice to requested page
        let runs = if start_idx < total_count {
            let end = end_idx.min(total_count);
            all_runs[start_idx..end].to_vec()
        } else {
            Vec::new()
        };

        eprintln!(
            "[PERF] Returned {} runs (of {} total, complete: {}, showing page {} of {})",
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
        let cached_pipelines = self.repository.get_cached_pipelines(None)?;
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
        let cached_pipelines = self.repository.get_cached_pipelines(None)?;
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
        let cached_pipelines = self.repository.get_cached_pipelines(None)?;
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
        let mut cache = self.run_history_cache.write().await;
        if cache.remove(pipeline_id).is_some() {
            eprintln!("[CACHE] Cleared cache for pipeline: {}", pipeline_id);
        }
    }

    /// Clear all run history caches
    pub async fn clear_all_run_history_caches(&self) {
        let mut cache = self.run_history_cache.write().await;
        let count = cache.len();
        cache.clear();
        eprintln!("[CACHE] Cleared {} cached run histories", count);
    }
}
