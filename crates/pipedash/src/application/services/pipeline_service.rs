use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tauri::Emitter;
use tokio::time::timeout;

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

    async fn update_provider_status_and_emit(
        &self, provider_id: i64, success: bool, error: Option<String>,
        app_handle: &Option<tauri::AppHandle>,
    ) {
        if self
            .repository
            .update_provider_fetch_status(provider_id, success, error)
            .await
            .is_ok()
        {
            if let Some(ref handle) = app_handle {
                let _ = handle.emit("providers-changed", ());
            }
        }
    }

    pub async fn fetch_pipelines(
        &self, provider_id: Option<i64>, app_handle: Option<tauri::AppHandle>,
    ) -> DomainResult<Vec<Pipeline>> {
        if let Err(_e) = self.repository.clear_workflow_parameters_cache().await {
            // Ignore cache clear errors
        }

        if let Some(pid) = provider_id {
            let provider = self.provider_service.get_provider(pid).await?;
            let result = provider.fetch_pipelines().await;

            match result {
                Ok(pipelines) => {
                    self.repository
                        .update_pipelines_cache(pid, &pipelines)
                        .await?;

                    self.update_provider_status_and_emit(pid, true, None, &app_handle)
                        .await;

                    Ok(pipelines)
                }
                Err(e) => {
                    let error_msg = format!("{e}");
                    self.update_provider_status_and_emit(pid, false, Some(error_msg), &app_handle)
                        .await;

                    Err(e)
                }
            }
        } else {
            let provider_summaries = self.provider_service.list_providers().await?;

            let futures: Vec<_> = provider_summaries
                .into_iter()
                .map(|summary| {
                    let provider_service = self.provider_service.clone();
                    let deduplicator = self.deduplicator.clone();
                    let repository = self.repository.clone();
                    let app_handle = app_handle.clone();
                    let provider_id = summary.id;
                    async move {
                        let request_id = hash_request(provider_id, "fetch_pipelines");

                        // Add 30 second timeout per provider to prevent hanging
                        // when a provider is disconnected or unresponsive
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
                                // Success - update status and return pipelines
                                if repository
                                    .update_provider_fetch_status(provider_id, true, None)
                                    .await
                                    .is_ok()
                                {
                                    if let Some(ref handle) = app_handle {
                                        let _ = handle.emit("providers-changed", ());
                                    }
                                }

                                Ok((provider_id, pipelines))
                            }
                            Ok(Err(e)) => {
                                // Provider returned an error
                                let error_msg = format!("{e}");
                                if repository
                                    .update_provider_fetch_status(
                                        provider_id,
                                        false,
                                        Some(error_msg),
                                    )
                                    .await
                                    .is_ok()
                                {
                                    if let Some(ref handle) = app_handle {
                                        let _ = handle.emit("providers-changed", ());
                                    }
                                }

                                Err(e)
                            }
                            Err(_elapsed) => {
                                // Timeout - provider took too long to respond
                                let error_msg =
                                    "Connection timeout - provider did not respond".to_string();
                                if repository
                                    .update_provider_fetch_status(
                                        provider_id,
                                        false,
                                        Some(error_msg.clone()),
                                    )
                                    .await
                                    .is_ok()
                                {
                                    if let Some(ref handle) = app_handle {
                                        let _ = handle.emit("providers-changed", ());
                                    }
                                }

                                Err(crate::domain::DomainError::ProviderError(error_msg))
                            }
                        }
                    }
                })
                .collect();

            let results = futures::future::join_all(futures).await;

            let mut all_pipelines = Vec::new();
            for result in results {
                match result {
                    Ok((provider_id, pipelines)) => {
                        self.repository
                            .update_pipelines_cache(provider_id, &pipelines)
                            .await?;
                        all_pipelines.extend(pipelines);
                    }
                    Err(_e) => {
                        // Error already handled and stored
                    }
                }
            }

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
        }

        Ok(api_runs)
    }

    pub async fn fetch_run_history_paginated(
        &self, pipeline_id: &str, page: usize, page_size: usize,
        app_handle: Option<tauri::AppHandle>,
    ) -> DomainResult<PaginatedRunHistory> {
        let start_idx = (page - 1) * page_size;
        let end_idx = start_idx + page_size;

        let db_cached_all = self
            .repository
            .get_cached_run_history(pipeline_id, 10000)
            .await?;

        if !db_cached_all.is_empty() {
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
                total_count.div_ceil(page_size) + 1
            };

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

        let all_runs = self.fetch_run_history(pipeline_id, fetch_limit).await?;
        let total_count = all_runs.len();
        let is_complete = total_count < fetch_limit;

        let repository = self.repository.clone();
        let pipeline_id_owned = pipeline_id.to_string();
        let runs_to_cache = all_runs.clone();
        tokio::spawn(async move {
            let _ = repository
                .cache_run_history(&pipeline_id_owned, &runs_to_cache)
                .await;
        });

        if let Some(metrics_service) = &self.metrics_service {
            let pipeline_id_clone = pipeline_id.to_string();
            let runs_clone = all_runs.clone();
            let metrics_service_clone = metrics_service.clone();
            let app_handle_clone = app_handle.clone();

            tokio::spawn(async move {
                if let Ok(count) = metrics_service_clone
                    .extract_and_store_metrics(&pipeline_id_clone, &runs_clone)
                    .await
                {
                    if count > 0 {
                        if let Some(handle) = app_handle_clone {
                            let _ = handle.emit("metrics-generated", &pipeline_id_clone);
                        }
                    }
                }
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

    pub async fn refresh_all(&self, app_handle: Option<tauri::AppHandle>) -> DomainResult<()> {
        self.fetch_pipelines(None, app_handle).await?;
        Ok(())
    }

    /// Clear run history cache for a specific pipeline (used on manual refresh)
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

    pub async fn invalidate_run_cache(&self, pipeline_id: &str) {
        let _ = self.repository.clear_cached_run_history(pipeline_id).await;
    }
}
