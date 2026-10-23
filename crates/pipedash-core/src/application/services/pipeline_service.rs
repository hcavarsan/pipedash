use std::collections::HashMap;
use std::future::Future;
use std::sync::{
    Arc,
    Mutex as StdMutex,
    PoisonError,
};
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use dashmap::DashMap;
use futures::future::{
    BoxFuture,
    FutureExt,
    Shared,
};
use tokio::sync::{
    watch,
    Mutex,
    OwnedSemaphorePermit,
    Semaphore,
};
use tokio::task::JoinSet;
use tokio::time::{
    timeout,
    timeout_at,
    Instant,
};

const MAX_CONCURRENT_PROVIDER_FETCHES: usize = 10;
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const FETCH_TIMEOUT_MESSAGE: &str = "Connection timeout - provider did not respond";
const FORCE_FEEDS_MIN_AGE: Duration = Duration::from_secs(15);
const OVERDUE_FACTOR: i64 = 3;
const OVERDUE_MIN_SUCCESSFUL_RUNS: usize = 3;
const OVERDUE_RECENT_RUNS: usize = 20;
const OVERDUE_CONCURRENT_CHECKS: usize = 4;

use super::health_service::{
    HealthHandle,
    HealthInput,
};
use super::metrics_service::MetricsService;
use super::provider_service::{
    ProviderLease,
    ProviderService,
};
use crate::application::fetch_scheduler::{
    ClaimKind,
    FetchClaim,
    FetchResult,
    FetchScheduler,
    ForcedClaim,
    Outcome,
};
use crate::application::refresh_manager::{
    RefreshDispatcher,
    RefreshMode,
};
use crate::domain::{
    DomainError,
    DomainResult,
    FailureKind,
    PaginatedRunHistory,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    RefreshResult,
    RefreshState,
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

type RefreshFlight = Shared<BoxFuture<'static, DomainResult<Vec<RefreshResult>>>>;

struct Applied {
    pipelines: Vec<Pipeline>,
    changed_ids: Vec<String>,
}

pub struct PipelineService {
    repository: Arc<Repository>,
    provider_service: Arc<ProviderService>,
    metrics_service: Option<Arc<MetricsService>>,
    event_bus: Arc<dyn EventBus>,
    run_deduplicator: Arc<RequestDeduplicator<Vec<PipelineRun>, DomainError>>,
    overdue_runs: Arc<DashMap<String, String>>,
    scheduler: Arc<FetchScheduler>,
    fetch_permits: Arc<Semaphore>,
    health: HealthHandle,
    mode: watch::Receiver<RefreshMode>,
    dispatcher: RefreshDispatcher,
    refresh_flight: Mutex<Option<RefreshFlight>>,
    background: StdMutex<JoinSet<()>>,
}

impl PipelineService {
    pub const REFRESH_DEADLINE: Duration = Duration::from_secs(25);

    pub fn new(
        repository: Arc<Repository>, provider_service: Arc<ProviderService>,
        metrics_service: Option<Arc<MetricsService>>, event_bus: Arc<dyn EventBus>,
        mode: watch::Receiver<RefreshMode>,
    ) -> Self {
        Self {
            repository,
            scheduler: Arc::clone(provider_service.scheduler()),
            health: provider_service.health().clone(),
            dispatcher: provider_service.dispatcher().clone(),
            provider_service,
            metrics_service,
            event_bus,
            run_deduplicator: Arc::new(RequestDeduplicator::new()),
            overdue_runs: Arc::new(DashMap::new()),
            fetch_permits: Arc::new(Semaphore::new(MAX_CONCURRENT_PROVIDER_FETCHES)),
            mode,
            refresh_flight: Mutex::new(None),
            background: StdMutex::new(JoinSet::new()),
        }
    }

    pub(crate) async fn claim_due(&self, now: Instant) -> Vec<FetchClaim> {
        self.provider_service.load_missing_providers().await;
        let floors = self.health.floors(self.scheduler.policy().outage_floor);

        self.scheduler.claim_due(now, &floors)
    }

    pub async fn fetch_provider(&self, claim: FetchClaim, kind: ClaimKind) -> FetchResult {
        let lease = match self.provider_service.acquire(claim.id()).await {
            Ok(lease) => lease,
            Err(error) => return self.fail_unloaded(claim, error).await,
        };
        if lease.generation != claim.generation() {
            claim.complete(Instant::now(), Outcome::Cancelled, FetchResult::Superseded);
            return FetchResult::Superseded;
        }
        let Some(permit) = self.acquire_permit(kind).await else {
            return FetchResult::Cancelled;
        };

        if self.provider_service.check_quota(&lease).await.is_some() {
            drop(permit);
            claim.complete(Instant::now(), Outcome::Cancelled, FetchResult::Cancelled);
            return FetchResult::Cancelled;
        }

        let fetched = match timeout(FETCH_TIMEOUT, lease.provider.fetch_pipelines()).await {
            Ok(fetched) => fetched,
            Err(_) => Err(DomainError::Timeout(FETCH_TIMEOUT_MESSAGE.to_string())),
        };
        drop(permit);

        let mut claim = Some(claim);
        let committed = self
            .provider_service
            .commit(&lease, || self.commit_fetch(&lease, fetched, &mut claim))
            .await;
        let result = match committed {
            Some(result) => result,
            None => {
                if let Some(claim) = claim {
                    claim.complete(Instant::now(), Outcome::Cancelled, FetchResult::Superseded);
                }
                FetchResult::Superseded
            }
        };

        if matches!(result, FetchResult::Committed(Ok(_))) {
            self.refresh_overdue(&lease).await;
        }

        result
    }

    async fn acquire_permit(&self, kind: ClaimKind) -> Option<OwnedSemaphorePermit> {
        let permits = Arc::clone(&self.fetch_permits);
        match kind {
            ClaimKind::Forced => permits.acquire_owned().await.ok(),
            ClaimKind::Scheduled => {
                let mut mode = self.mode.clone();
                let idle = async {
                    mode.wait_for(|mode| *mode == RefreshMode::Idle)
                        .await
                        .is_ok()
                };
                tokio::select! {
                    biased;
                    true = idle => None,
                    permit = permits.acquire_owned() => permit.ok(),
                }
            }
        }
    }

    async fn fail_unloaded(&self, claim: FetchClaim, error: DomainError) -> FetchResult {
        if matches!(error, DomainError::ProviderNotFound(_)) {
            claim.complete(Instant::now(), Outcome::Cancelled, FetchResult::Superseded);
            return FetchResult::Superseded;
        }

        if self
            .write_status(claim.id(), false, Some(error.to_string()))
            .await
        {
            self.event_bus.emit(CoreEvent::ProvidersChanged).await;
        }
        let outcome = failed_outcome(&error);
        let result = FetchResult::Committed(Err(error));
        claim.complete(Instant::now(), outcome, result.clone());

        result
    }

    async fn commit_fetch(
        &self, lease: &ProviderLease, fetched: DomainResult<Vec<Pipeline>>,
        claim: &mut Option<FetchClaim>,
    ) -> FetchResult {
        let id = lease.id;
        let applied = match fetched {
            Ok(pipelines) => self.apply_pipelines(lease, pipelines).await,
            Err(error) => Err(error),
        };
        let status_changed = match &applied {
            Ok(_) => self.write_status(id, true, None).await,
            Err(error) => self.write_status(id, false, Some(error.to_string())).await,
        };

        let mut content_changed = false;
        let (outcome, result) = match applied {
            Ok(applied) => {
                content_changed = !applied.changed_ids.is_empty();
                let count = applied.pipelines.len();
                self.emit_fetch_events(id, applied).await;
                (Outcome::Success, FetchResult::Committed(Ok(count)))
            }
            Err(error) => (failed_outcome(&error), FetchResult::Committed(Err(error))),
        };
        let kind = match &result {
            FetchResult::Committed(Err(error)) => Some(error.failure_kind()),
            _ => None,
        };

        let before = self.scheduler.view(id, Instant::now());
        let view = claim
            .take()
            .and_then(|claim| claim.complete(Instant::now(), outcome, result.clone()));
        if let Some(view) = view {
            let changed = status_changed
                || content_changed
                || self.provider_service.published_in_flight(id)
                || before.is_none_or(|before| {
                    before.failures != view.failures || before.last_failure != view.last_failure
                });
            self.health.send(HealthInput::FetchOutcome {
                provider_id: id,
                generation: lease.generation,
                view,
                kind,
            });
            if changed {
                self.provider_service.emit_status(id).await;
            }
        }

        result
    }

    async fn apply_pipelines(
        &self, lease: &ProviderLease, pipelines: Vec<Pipeline>,
    ) -> DomainResult<Applied> {
        let id = lease.id;
        let pipelines = self.mark_overdue(pipelines);
        let old = self.repository.get_cached_pipelines(Some(id)).await?;
        self.repository
            .update_pipelines_cache(id, &pipelines)
            .await?;

        let old_by_id: HashMap<&str, &Pipeline> = old
            .iter()
            .map(|pipeline| (pipeline.id.as_str(), pipeline))
            .collect();
        let retains_run_history = lease.provider.capabilities().retains_run_history;
        let mut changed_ids = Vec::new();
        for pipeline in &pipelines {
            match old_by_id.get(pipeline.id.as_str()) {
                Some(previous)
                    if previous.status == pipeline.status
                        && previous.last_run == pipeline.last_run => {}
                Some(_) => {
                    changed_ids.push(pipeline.id.clone());
                    if !retains_run_history {
                        tracing::debug!(pipeline = %pipeline.name, "Pipeline changed, invalidating run cache");
                        self.invalidate_run_cache(&pipeline.id).await;
                    }
                }
                None => changed_ids.push(pipeline.id.clone()),
            }
        }
        let current: std::collections::HashSet<&str> = pipelines
            .iter()
            .map(|pipeline| pipeline.id.as_str())
            .collect();
        changed_ids.extend(
            old.iter()
                .filter(|pipeline| !current.contains(pipeline.id.as_str()))
                .map(|pipeline| pipeline.id.clone()),
        );

        Ok(Applied {
            pipelines,
            changed_ids,
        })
    }

    async fn emit_fetch_events(&self, id: i64, applied: Applied) {
        self.event_bus
            .emit(CoreEvent::PipelinesUpdated {
                pipelines: applied.pipelines,
                provider_id: id,
                timestamp: Utc::now().timestamp_millis(),
            })
            .await;
        self.event_bus
            .emit(CoreEvent::PipelineCacheInvalidated {
                provider_id: Some(id),
                reason: CacheInvalidationReason::Fetch,
            })
            .await;
        if !applied.changed_ids.is_empty() {
            self.event_bus
                .emit(CoreEvent::PipelineStatusChanged {
                    pipeline_ids: applied.changed_ids,
                })
                .await;
        }
    }

    async fn write_status(&self, id: i64, success: bool, error: Option<String>) -> bool {
        match self
            .repository
            .update_provider_fetch_status(id, success, error)
            .await
        {
            Ok(changed) => changed,
            Err(e) => {
                tracing::warn!(provider_id = id, error = %e, "Failed to store provider fetch status");
                false
            }
        }
    }

    async fn refresh_overdue(&self, lease: &ProviderLease) {
        if !self.update_overdue_pipelines(lease.id).await {
            return;
        }

        self.provider_service
            .commit(lease, || async {
                match self.get_cached_pipelines(Some(lease.id)).await {
                    Ok(pipelines) => {
                        self.event_bus
                            .emit(CoreEvent::PipelinesUpdated {
                                pipelines,
                                provider_id: lease.id,
                                timestamp: Utc::now().timestamp_millis(),
                            })
                            .await;
                    }
                    Err(e) => {
                        tracing::warn!(provider_id = lease.id, error = %e, "Failed to publish overdue pipelines");
                    }
                }
            })
            .await;
    }

    pub async fn fetch_now(&self, ids: &[i64], deadline: Duration) -> Vec<RefreshResult> {
        let deadline_at = Instant::now() + deadline;

        futures::future::join_all(ids.iter().map(|id| async move {
            match timeout_at(deadline_at, self.fetch_one(*id)).await {
                Ok(result) => result,
                Err(_) => RefreshResult::new(*id, RefreshState::Pending),
            }
        }))
        .await
    }

    pub async fn refresh_provider(&self, id: i64) -> RefreshResult {
        let mut results = self.fetch_now(&[id], Self::REFRESH_DEADLINE).await;

        results
            .pop()
            .unwrap_or_else(|| RefreshResult::new(id, RefreshState::Pending))
    }

    async fn fetch_one(&self, id: i64) -> RefreshResult {
        let mut superseded = false;
        loop {
            let lease = match self.provider_service.acquire(id).await {
                Ok(lease) => lease,
                Err(DomainError::ProviderNotFound(_)) => return not_found(id),
                Err(error) => {
                    return RefreshResult::failed(id, error.failure_kind(), error.to_string())
                }
            };
            let handle = match self
                .scheduler
                .claim_forced(id, lease.generation, Instant::now())
            {
                ForcedClaim::Claimed(claim) => {
                    let handle = claim.handle();
                    self.dispatcher.fetch_now(vec![claim]);
                    handle
                }
                ForcedClaim::Joined(handle) => handle,
                ForcedClaim::Recent(outcome) => {
                    return RefreshResult {
                        provider_id: id,
                        state: RefreshState::Recent,
                        kind: match outcome {
                            Outcome::Failed { kind, .. } => Some(kind),
                            Outcome::Success | Outcome::Cancelled => None,
                        },
                        message: None,
                    };
                }
                ForcedClaim::Refused => return not_found(id),
            };

            match handle.wait().await {
                FetchResult::Committed(Ok(_)) => return RefreshResult::new(id, RefreshState::Ok),
                FetchResult::Committed(Err(error)) => {
                    return RefreshResult::failed(id, error.failure_kind(), error.to_string())
                }
                FetchResult::Superseded if !superseded => superseded = true,
                FetchResult::Cancelled if self.scheduler.paused_until(id, Utc::now()).is_some() => {
                    return RefreshResult {
                        provider_id: id,
                        state: RefreshState::Recent,
                        kind: Some(FailureKind::RateLimited),
                        message: None,
                    };
                }
                FetchResult::Superseded | FetchResult::Cancelled => {
                    return RefreshResult::new(id, RefreshState::Pending)
                }
            }
        }
    }

    pub async fn refresh_all(self: &Arc<Self>) -> DomainResult<Vec<RefreshResult>> {
        let flight = {
            let mut slot = self.refresh_flight.lock().await;
            match slot.as_ref().filter(|flight| flight.peek().is_none()) {
                Some(flight) => flight.clone(),
                None => {
                    let service = Arc::clone(self);
                    let task = tokio::spawn(async move { service.run_refresh_all().await });
                    let flight: RefreshFlight = async move {
                        task.await
                            .unwrap_or_else(|e| Err(DomainError::InternalError(e.to_string())))
                    }
                    .boxed()
                    .shared();
                    *slot = Some(flight.clone());
                    flight
                }
            }
        };

        flight.await
    }

    async fn run_refresh_all(&self) -> DomainResult<Vec<RefreshResult>> {
        let providers = self.repository.config_backend().list_providers().await?;
        for provider in &providers {
            let Some(id) = provider.id else {
                continue;
            };
            if !self
                .provider_service
                .retains_run_history(&provider.provider_type)
            {
                if let Err(e) = self.repository.clear_provider_run_history(id).await {
                    tracing::warn!(provider_id = id, error = %e, "Failed to clear run history before refresh");
                }
            }
        }
        self.health.send(HealthInput::ForceFeeds {
            min_age: FORCE_FEEDS_MIN_AGE,
        });
        let ids: Vec<i64> = providers
            .iter()
            .filter_map(|provider| provider.id)
            .collect();

        Ok(self.fetch_now(&ids, Self::REFRESH_DEADLINE).await)
    }

    pub async fn get_cached_pipelines(
        &self, provider_id: Option<i64>,
    ) -> DomainResult<Vec<Pipeline>> {
        Ok(self.mark_overdue(self.repository.get_cached_pipelines(provider_id).await?))
    }

    fn mark_overdue(&self, mut pipelines: Vec<Pipeline>) -> Vec<Pipeline> {
        for pipeline in &mut pipelines {
            pipeline.overdue_run = (pipeline.status == PipelineStatus::Running)
                .then(|| self.overdue_runs.get(&pipeline.id).map(|run| run.clone()))
                .flatten();
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
        let permits = Semaphore::new(OVERDUE_CONCURRENT_CHECKS);
        let checks = pipelines
            .iter()
            .filter(|p| p.status == PipelineStatus::Running && p.provider_type != "fluxcd")
            .map(|pipeline| {
                let permits = &permits;
                async move {
                    let _permit = permits.acquire().await.ok()?;
                    match self.recent_runs(&pipeline.id).await {
                        Ok(runs) => overdue_run(&runs, now).map(|run| (pipeline.id.clone(), run)),
                        Err(e) => {
                            tracing::debug!(pipeline_id = %pipeline.id, error = %e, "Overdue check failed");
                            None
                        }
                    }
                }
            });
        let overdue: HashMap<String, String> = futures::future::join_all(checks)
            .await
            .into_iter()
            .flatten()
            .collect();

        let mut changed = false;
        self.overdue_runs.retain(|id, run| {
            let keep = provider_id_of(id) != Some(provider_id) || overdue.get(id) == Some(run);
            changed |= !keep;
            keep
        });
        for (id, run) in overdue {
            changed |= self.overdue_runs.insert(id, run).is_none();
        }

        changed
    }

    async fn recent_runs(&self, pipeline_id: &str) -> DomainResult<Vec<PipelineRun>> {
        if self.repository.get_cached_run_count(pipeline_id).await? > 0 {
            return self
                .repository
                .get_paginated_runs(pipeline_id, 1, OVERDUE_RECENT_RUNS)
                .await;
        }

        Ok(self
            .fetch_run_history_paginated(pipeline_id, 1, OVERDUE_RECENT_RUNS)
            .await?
            .runs)
    }

    pub async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> DomainResult<Vec<PipelineRun>> {
        Ok(self.fetch_run_history_leased(pipeline_id, limit).await?.0)
    }

    async fn fetch_run_history_leased(
        &self, pipeline_id: &str, limit: usize,
    ) -> DomainResult<(Vec<PipelineRun>, ProviderLease)> {
        let cached_pipelines = self.repository.get_cached_pipelines(None).await?;
        let pipeline = cached_pipelines
            .iter()
            .find(|p| p.id == pipeline_id)
            .ok_or_else(|| DomainError::PipelineNotFound(pipeline_id.to_string()))?;

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
            .commit(&lease, || self.merge_run_cache(pipeline, &api_runs))
            .await
            .transpose()?;

        Ok((api_runs, lease))
    }

    async fn merge_run_cache(
        &self, pipeline: &Pipeline, api_runs: &[PipelineRun],
    ) -> DomainResult<()> {
        let pipeline_id = pipeline.id.as_str();
        let cached_with_hashes = self
            .repository
            .get_cached_runs_with_hashes(pipeline_id)
            .await?;

        let mut api_map: HashMap<i64, (PipelineRun, String)> = HashMap::new();
        for run in api_runs {
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
        let bounded_history = pipeline.provider_type == "fluxcd";
        for (run_number, (cached_run, _)) in &cached_with_hashes {
            let snapshot = cached_run
                .metadata
                .get("snapshot")
                .and_then(serde_json::Value::as_bool)
                == Some(true);
            if api_map.contains_key(run_number) {
                continue;
            }
            if bounded_history && is_archivable_flux_run(pipeline, cached_run) {
                if cached_run.metadata.get("archived") != Some(&serde_json::Value::Bool(true)) {
                    let mut archived = cached_run.clone();
                    archived
                        .metadata
                        .insert("archived".to_string(), serde_json::Value::Bool(true));
                    changed_runs.push(archived);
                }
            } else if *run_number >= min_api_run_number || snapshot || bounded_history {
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

        Ok(())
    }

    fn spawn_metrics(
        &self, lease: ProviderLease, pipeline_id: String, runs: Option<Vec<PipelineRun>>,
    ) {
        let Some(metrics_service) = self.metrics_service.clone() else {
            return;
        };
        let provider_service = Arc::clone(&self.provider_service);
        let repository = Arc::clone(&self.repository);
        let event_bus = Arc::clone(&self.event_bus);

        self.spawn_background(async move {
            provider_service
                .commit(&lease, || async {
                    let runs = match runs {
                        Some(runs) => runs,
                        None => {
                            match repository.get_cached_run_history(&pipeline_id, 10000).await {
                                Ok(runs) => runs,
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        pipeline_id = %pipeline_id,
                                        "Failed to load cached runs for metrics extraction"
                                    );
                                    return;
                                }
                            }
                        }
                    };
                    match metrics_service
                        .extract_and_store_metrics(&pipeline_id, &runs)
                        .await
                    {
                        Ok(count) if count > 0 => {
                            event_bus
                                .emit(CoreEvent::MetricsGenerated {
                                    pipeline_id: pipeline_id.clone(),
                                })
                                .await;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            tracing::error!(
                                error = %e,
                                pipeline_id = %pipeline_id,
                                "Failed to extract and store metrics"
                            );
                        }
                    }
                })
                .await;
        });
    }

    fn spawn_background(&self, task: impl Future<Output = ()> + Send + 'static) {
        let mut background = self
            .background
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        while background.try_join_next().is_some() {}
        background.spawn(task);
    }

    #[cfg(test)]
    pub(crate) async fn settle(&self) {
        let mut tasks = std::mem::take(
            &mut *self
                .background
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        while tasks.join_next().await.is_some() {}
    }

    pub async fn fetch_run_history_paginated(
        &self, pipeline_id: &str, page: usize, page_size: usize,
    ) -> DomainResult<PaginatedRunHistory> {
        let params = pipedash_plugin_api::PaginationParams { page, page_size };
        params.validate().map_err(DomainError::InvalidConfig)?;
        let start_idx = params
            .calculate_offset()
            .map_err(DomainError::InvalidConfig)?;
        let end_idx = start_idx
            .checked_add(page_size)
            .ok_or_else(|| DomainError::InvalidConfig("Pagination offset overflow".to_string()))?;
        let bounded_history = pipeline_id.starts_with("fluxcd__");

        let cached_count = self.repository.get_cached_run_count(pipeline_id).await?;

        if !bounded_history && cached_count >= end_idx {
            let runs = self
                .repository
                .get_paginated_runs(pipeline_id, page, page_size)
                .await?;

            let is_complete = false;
            let total_pages = cached_count.div_ceil(page_size);

            if page == 1 {
                if let Some(provider_id) = provider_id_of(pipeline_id) {
                    if let Ok(lease) = self.provider_service.acquire(provider_id).await {
                        self.spawn_metrics(lease, pipeline_id.to_string(), None);
                    }
                }
            }

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

        let needed_runs = end_idx;

        let github_pages_needed = (needed_runs / 100) + 1;
        let fetch_limit = if bounded_history {
            usize::MAX
        } else {
            github_pages_needed.min(10) * 100
        };

        tracing::debug!(
            page = page,
            page_size = page_size,
            needed_runs = needed_runs,
            fetch_limit = fetch_limit,
            "Smart pagination: fetching only what's needed"
        );

        let (all_runs, lease) = self
            .fetch_run_history_leased(pipeline_id, fetch_limit)
            .await?;
        let total_count = all_runs.len();

        let is_complete = total_count < fetch_limit;

        self.spawn_metrics(lease, pipeline_id.to_string(), Some(all_runs.clone()));

        if bounded_history {
            let total_count = self.repository.get_cached_run_count(pipeline_id).await?;
            let runs = self
                .repository
                .get_paginated_runs(pipeline_id, page, page_size)
                .await?;
            return Ok(PaginatedRunHistory {
                runs,
                total_count,
                has_more: end_idx < total_count,
                is_complete: true,
                page,
                page_size,
                total_pages: total_count.div_ceil(page_size),
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
        match provider.fetch_run_details(pipeline_id, run_number).await {
            Err(error @ DomainError::PipelineNotFound(_)) if pipeline.provider_type == "fluxcd" => {
                let cached = self
                    .repository
                    .get_cached_run(pipeline_id, run_number)
                    .await?;
                cached
                    .and_then(|run| archived_flux_run(pipeline, run))
                    .ok_or(error)
            }
            result => result,
        }
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

    pub async fn review_run(
        &self, pipeline_id: &str, run_number: i64, decision: pipedash_plugin_api::ReviewDecision,
    ) -> DomainResult<()> {
        let cached_pipelines = self.repository.get_cached_pipelines(None).await?;
        let pipeline = cached_pipelines
            .iter()
            .find(|p| p.id == pipeline_id)
            .ok_or_else(|| DomainError::PipelineNotFound(pipeline_id.to_string()))?;

        let provider = self
            .provider_service
            .get_provider(pipeline.provider_id)
            .await?;
        provider
            .review_run(pipeline_id, run_number, decision)
            .await?;

        self.invalidate_run_cache(pipeline_id).await;

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

fn failed_outcome(error: &DomainError) -> Outcome {
    Outcome::Failed {
        kind: error.failure_kind(),
        retry_after: error.retry_after(),
    }
}

fn not_found(provider_id: i64) -> RefreshResult {
    RefreshResult {
        provider_id,
        state: RefreshState::NotFound,
        kind: None,
        message: Some("Provider not found".to_string()),
    }
}

fn provider_id_of(pipeline_id: &str) -> Option<i64> {
    pipeline_id.split("__").nth(1)?.parse().ok()
}

fn overdue_run(runs: &[PipelineRun], now: DateTime<Utc>) -> Option<String> {
    let running = runs
        .iter()
        .find(|run| run.status == PipelineStatus::Running)?;
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
    let median = if durations.len() % 2 == 0 {
        (durations[middle - 1] + durations[middle]) / 2
    } else {
        durations[middle]
    };

    ((now - running.started_at).num_seconds() > OVERDUE_FACTOR * median).then(|| running.id.clone())
}

fn is_archivable_flux_run(pipeline: &Pipeline, run: &PipelineRun) -> bool {
    let Some(uid) = pipeline
        .metadata
        .get("uid")
        .and_then(serde_json::Value::as_str)
    else {
        return false;
    };
    run.metadata.get("uid").and_then(serde_json::Value::as_str) == Some(uid)
        && run
            .metadata
            .get("record_source")
            .and_then(serde_json::Value::as_str)
            == Some("status.history")
        && !matches!(
            run.status,
            crate::domain::PipelineStatus::Running | crate::domain::PipelineStatus::Pending
        )
}

fn archived_flux_run(pipeline: &Pipeline, mut run: PipelineRun) -> Option<PipelineRun> {
    if !is_archivable_flux_run(pipeline, &run) {
        return None;
    }
    run.metadata
        .insert("archived".to_string(), serde_json::Value::Bool(true));
    Some(run)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::application::test_support::{
        pipeline,
        stub_run,
        Gate,
        Harness,
        StubProvider,
    };
    use crate::domain::{
        FetchStatus,
        ProviderConfig,
    };

    fn history_fixture() -> (Pipeline, PipelineRun) {
        let pipeline = serde_json::from_value(json!({
            "id": "fluxcd__1__HelmRelease__apps__web", "provider_id": 1,
            "provider_type": "fluxcd", "name": "web", "status": "success",
            "repository": "apps/charts", "last_updated": "2026-10-21T00:00:00Z",
            "metadata": { "uid": "resource-1" }
        }))
        .unwrap();
        let run = serde_json::from_value(json!({
            "id": "release-1", "pipeline_id": "fluxcd__1__HelmRelease__apps__web",
            "run_number": 1, "status": "success", "started_at": "2026-10-20T00:00:00Z",
            "logs_url": "", "metadata": { "uid": "resource-1", "record_source": "status.history", "release_version": 1 }
        })).unwrap();
        (pipeline, run)
    }

    #[test]
    fn retained_native_history_is_marked_as_archived() {
        let (pipeline, run) = history_fixture();
        let archived = archived_flux_run(&pipeline, run).unwrap();
        assert_eq!(archived.id, "release-1");
        assert_eq!(archived.metadata["archived"], json!(true));
        assert_eq!(archived.metadata["release_version"], json!(1));
    }

    #[test]
    fn recreated_resources_do_not_reuse_old_history() {
        let (mut pipeline, run) = history_fixture();
        pipeline
            .metadata
            .insert("uid".to_string(), json!("resource-2"));
        assert!(archived_flux_run(&pipeline, run).is_none());
    }

    #[test]
    fn current_state_snapshots_are_not_archived_as_executions() {
        let (pipeline, mut run) = history_fixture();
        run.metadata
            .insert("record_source".to_string(), json!("current_snapshot"));
        assert!(archived_flux_run(&pipeline, run).is_none());
    }

    #[test]
    fn unfinished_history_is_not_presented_as_a_final_state() {
        let (pipeline, mut run) = history_fixture();
        run.status = PipelineStatus::Running;
        assert!(archived_flux_run(&pipeline, run).is_none());
    }

    fn run(
        number: i64, status: &str, started_minutes_ago: i64, minutes: Option<i64>,
    ) -> PipelineRun {
        serde_json::from_value(json!({
            "id": number.to_string(), "pipeline_id": "jenkins__1__build", "run_number": number,
            "status": status, "started_at": Utc::now() - chrono::Duration::minutes(started_minutes_ago),
            "duration_seconds": minutes.map(|m| m * 60), "logs_url": ""
        }))
        .unwrap()
    }

    fn ten_minute_history(running_for_minutes: i64) -> Vec<PipelineRun> {
        vec![
            run(5, "running", running_for_minutes, None),
            run(4, "success", 120, Some(10)),
            run(3, "failed", 180, Some(90)),
            run(2, "success", 240, Some(9)),
            run(1, "success", 300, Some(11)),
        ]
    }

    #[test]
    fn a_run_is_overdue_past_three_times_the_median_success() {
        let now = Utc::now();

        assert_eq!(
            overdue_run(&ten_minute_history(45), now),
            Some("5".to_string())
        );
        assert_eq!(overdue_run(&ten_minute_history(25), now), None);
        assert_eq!(overdue_run(&ten_minute_history(45)[..3], now), None);
        assert_eq!(overdue_run(&ten_minute_history(45)[1..], now), None);
    }

    #[tokio::test]
    async fn refresh_flags_a_running_pipeline_far_past_its_usual_duration() {
        let h = Harness::with_real_plugins().await;
        let repository = &h.repository;
        let provider_id = repository
            .add_provider(&ProviderConfig {
                id: None,
                name: "jenkins".into(),
                provider_type: "jenkins".into(),
                token: "token".into(),
                config: HashMap::new(),
                refresh_interval: 30,
                version: None,
            })
            .await
            .unwrap();
        let pipeline: Pipeline = serde_json::from_value(json!({
            "id": "jenkins__1__build", "provider_id": provider_id, "provider_type": "jenkins",
            "name": "build", "status": "running", "repository": "build",
            "last_updated": Utc::now()
        }))
        .unwrap();
        repository
            .update_pipelines_cache(provider_id, &[pipeline])
            .await
            .unwrap();
        repository
            .cache_run_history("jenkins__1__build", &ten_minute_history(45))
            .await
            .unwrap();
        let service = &h.pipeline_service;

        let before = service.get_cached_pipelines(None).await.unwrap();
        let changed = service.update_overdue_pipelines(provider_id).await;
        let unchanged = service.update_overdue_pipelines(provider_id).await;
        let after = service.get_cached_pipelines(None).await.unwrap();
        repository
            .clear_cached_run_history("jenkins__1__build")
            .await
            .unwrap();
        let after_failed_lookup = service.update_overdue_pipelines(provider_id).await;
        let unknown = service.get_cached_pipelines(None).await.unwrap();
        repository
            .cache_run_history("jenkins__1__build", &ten_minute_history(45))
            .await
            .unwrap();
        service.update_overdue_pipelines(provider_id).await;
        let again = service.get_cached_pipelines(None).await.unwrap();

        assert_eq!(before[0].overdue_run, None);
        assert!(changed);
        assert!(!unchanged);
        assert_eq!(after[0].overdue_run.as_deref(), Some("5"));
        assert!(after_failed_lookup);
        assert_eq!(unknown[0].overdue_run, None);
        assert_eq!(again[0].overdue_run.as_deref(), Some("5"));
    }

    #[tokio::test]
    async fn a_hanging_provider_does_not_lose_another_providers_result() {
        let h = Harness::new().await;
        h.start().await;
        let gate = Gate::new();
        let (hanging, _) = h.add_stub(|id| StubProvider::new(id).gated(&gate)).await;
        let (healthy, _) = h.add_stub(StubProvider::new).await;

        let results = h
            .pipeline_service
            .fetch_now(&[hanging, healthy], Duration::from_secs(2))
            .await;
        let summaries = h.provider_service.list_providers().await.unwrap();
        let summary = |id| summaries.iter().find(|summary| summary.id == id).unwrap();

        assert_eq!(results[0].state, RefreshState::Pending);
        assert_eq!(results[1].state, RefreshState::Ok);
        assert_eq!(h.cached_names(healthy).await, ["web"]);
        assert!(h.cached_names(hanging).await.is_empty());
        assert!(matches!(
            summary(healthy).last_fetch_status,
            FetchStatus::Success
        ));
        assert!(matches!(
            summary(hanging).last_fetch_status,
            FetchStatus::Never
        ));
        assert!(summary(hanging).health.in_flight);

        gate.open();
        h.bus
            .wait_for("the late result", |events| {
                events.iter().any(|event| {
                    matches!(event, CoreEvent::PipelinesUpdated { provider_id, .. } if *provider_id == hanging)
                })
            })
            .await;
        assert_eq!(h.cached_names(hanging).await, ["web"]);
    }

    #[tokio::test]
    async fn a_failed_cache_write_is_never_reported_as_a_successful_fetch() {
        let h = Harness::new().await;
        let (id, _) = h.add_stub(StubProvider::new).await;
        sqlx::query("DROP TABLE pipelines_cache")
            .execute(&h.pool)
            .await
            .unwrap();

        let result = h.fetch_directly(id).await;
        let (status, error, _) = h.repository.get_provider_fetch_status(id).await.unwrap();

        assert!(matches!(result, FetchResult::Committed(Err(_))));
        assert_eq!(status, "error");
        assert!(error.is_some());
        assert_eq!(h.bus.count("pipelines-updated"), 0);
    }

    #[tokio::test]
    async fn a_manual_refresh_during_a_scheduled_attempt_joins_it() {
        let h = Harness::new().await;
        let gate = Gate::new();
        let (id, stub) = h.add_stub(|id| StubProvider::new(id).gated(&gate)).await;
        h.provider_service.acquire(id).await.unwrap();
        let claim = h.pipeline_service.claim_due(Instant::now()).await.remove(0);
        let service = Arc::clone(&h.pipeline_service);
        let scheduled =
            tokio::spawn(async move { service.fetch_provider(claim, ClaimKind::Scheduled).await });
        gate.arrived().await;

        let service = Arc::clone(&h.pipeline_service);
        let manual =
            tokio::spawn(async move { service.fetch_now(&[id], Duration::from_secs(5)).await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        gate.open();

        let results = manual.await.unwrap();
        assert!(matches!(
            scheduled.await.unwrap(),
            FetchResult::Committed(Ok(1))
        ));
        assert_eq!(results[0].state, RefreshState::Ok);
        assert_eq!(stub.fetch_count(), 1);
    }

    #[tokio::test]
    async fn going_idle_stops_a_scheduled_fetch_waiting_for_a_permit() {
        let h = Harness::new().await;
        let (id, stub) = h.add_stub(StubProvider::new).await;
        h.provider_service.acquire(id).await.unwrap();
        let permits = Arc::clone(&h.pipeline_service.fetch_permits)
            .acquire_many_owned(MAX_CONCURRENT_PROVIDER_FETCHES as u32)
            .await
            .unwrap();
        let claim = h.pipeline_service.claim_due(Instant::now()).await.remove(0);
        let service = Arc::clone(&h.pipeline_service);
        let waiting =
            tokio::spawn(async move { service.fetch_provider(claim, ClaimKind::Scheduled).await });
        tokio::time::sleep(Duration::from_millis(100)).await;

        h.refresh_manager.set_mode(RefreshMode::Idle);
        drop(permits);

        assert!(matches!(waiting.await.unwrap(), FetchResult::Cancelled));
        assert_eq!(stub.fetch_count(), 0);
        let view = h.scheduler.view(id, Instant::now()).unwrap();
        assert!(!view.in_flight);
        assert_eq!(view.failures, 0);
        assert!(matches!(
            h.fetch_directly(id).await,
            FetchResult::Committed(Ok(1))
        ));
        assert_eq!(stub.fetch_count(), 1);
    }

    #[tokio::test]
    async fn a_second_refresh_all_joins_the_running_one() {
        let h = Harness::new().await;
        h.start().await;
        let gate = Gate::new();
        let (_, stub) = h.add_stub(|id| StubProvider::new(id).gated(&gate)).await;
        let first = tokio::spawn({
            let service = Arc::clone(&h.pipeline_service);
            async move { service.refresh_all().await }
        });
        gate.arrived().await;
        let second = tokio::spawn({
            let service = Arc::clone(&h.pipeline_service);
            async move { service.refresh_all().await }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        gate.open();

        let first = first.await.unwrap().unwrap();
        let second = second.await.unwrap().unwrap();
        let third = h.pipeline_service.refresh_all().await.unwrap();

        assert_eq!(first, second);
        assert_eq!(first[0].state, RefreshState::Ok);
        assert_eq!(third[0].state, RefreshState::Recent);
        assert_eq!(stub.fetch_count(), 1);
    }

    #[tokio::test]
    async fn refresh_all_keeps_the_run_history_of_plugins_that_retain_it() {
        let h = Harness::new().await;
        h.start().await;
        let mut ids = Vec::new();
        for provider_type in ["argocd", "fluxcd"] {
            let mut config = h.config(provider_type, 30);
            config.provider_type = provider_type.to_string();
            let id = h.repository.add_provider(&config).await.unwrap();
            h.stubs.queue(StubProvider::new(id));
            let pipeline_id = format!("github__{id}__web");
            h.repository
                .update_pipelines_cache(id, &[pipeline(id, "web", PipelineStatus::Success)])
                .await
                .unwrap();
            h.repository
                .cache_run_history(&pipeline_id, &[stub_run(&pipeline_id, 1, 60)])
                .await
                .unwrap();
            ids.push(pipeline_id);
        }

        let results = h.pipeline_service.refresh_all().await.unwrap();

        assert!(results
            .iter()
            .all(|result| result.state == RefreshState::Ok));
        assert_eq!(h.repository.get_cached_run_count(&ids[0]).await.unwrap(), 0);
        assert_eq!(h.repository.get_cached_run_count(&ids[1]).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn a_changed_pipeline_clears_its_run_cache_unless_the_plugin_retains_history() {
        let h = Harness::new().await;
        let (plain, plain_stub) = h.add_stub(StubProvider::new).await;
        let (retaining, retaining_stub) = h
            .add_stub(|id| StubProvider::new(id).retaining_run_history())
            .await;
        let mut cached = Vec::new();
        for (id, stub) in [(plain, &plain_stub), (retaining, &retaining_stub)] {
            h.fetch_directly(id).await;
            let pipeline_id = format!("github__{id}__web");
            h.repository
                .cache_run_history(&pipeline_id, &[stub_run(&pipeline_id, 1, 60)])
                .await
                .unwrap();
            stub.set_pipelines(vec![pipeline(id, "web", PipelineStatus::Failed)]);
            h.fetch_directly(id).await;
            cached.push(
                h.repository
                    .get_cached_run_count(&pipeline_id)
                    .await
                    .unwrap(),
            );
        }

        assert_eq!(cached, [0, 1]);
    }

    #[tokio::test]
    async fn a_fetch_publishes_its_provider_and_only_reports_real_changes() {
        let h = Harness::new().await;
        let (id, stub) = h.add_stub(StubProvider::new).await;
        let web = format!("github__{id}__web");
        let api = format!("github__{id}__api");

        h.fetch_directly(id).await;
        h.fetch_directly(id).await;
        stub.set_pipelines(vec![
            pipeline(id, "web", PipelineStatus::Failed),
            pipeline(id, "api", PipelineStatus::Success),
        ]);
        h.fetch_directly(id).await;
        stub.set_pipelines(vec![pipeline(id, "web", PipelineStatus::Failed)]);
        h.fetch_directly(id).await;

        let events = h.bus.events();
        let changed: Vec<Vec<String>> = events
            .iter()
            .filter_map(|event| match event {
                CoreEvent::PipelineStatusChanged { pipeline_ids } => {
                    let mut ids = pipeline_ids.clone();
                    ids.sort();
                    Some(ids)
                }
                _ => None,
            })
            .collect();
        let updated: Vec<i64> = events
            .iter()
            .filter_map(|event| match event {
                CoreEvent::PipelinesUpdated { provider_id, .. } => Some(*provider_id),
                _ => None,
            })
            .collect();

        assert_eq!(updated, vec![id; 4]);
        assert_eq!(h.bus.count("pipeline-cache-invalidated"), 4);
        assert_eq!(
            changed,
            vec![vec![web.clone()], vec![api.clone(), web.clone()], vec![api]]
        );
    }

    #[tokio::test]
    async fn a_failed_fetch_keeps_the_cache_and_records_the_failure() {
        let h = Harness::new().await;
        let (id, stub) = h.add_stub(StubProvider::new).await;
        h.fetch_directly(id).await;
        h.health_inputs();
        *stub.pipelines.lock().unwrap() = Err(DomainError::RateLimited {
            message: "slow down".to_string(),
            retry_after: Some(Duration::from_secs(120)),
        });

        let result = h.fetch_directly(id).await;
        let view = h.scheduler.view(id, Instant::now()).unwrap();
        let summary = &h.provider_service.list_providers().await.unwrap()[0];
        let inputs = h.health_inputs();

        assert!(matches!(
            result,
            FetchResult::Committed(Err(DomainError::RateLimited { .. }))
        ));
        assert_eq!(h.cached_names(id).await, ["web"]);
        assert_eq!(view.failures, 1);
        assert!(view.next_attempt_in >= Duration::from_secs(119));
        assert!(matches!(summary.last_fetch_status, FetchStatus::Error));
        assert_eq!(
            summary.last_fetch_error_kind,
            Some(crate::domain::FailureKind::RateLimited)
        );
        assert!(summary.health.next_attempt_at.is_some());
        assert!(matches!(
            inputs.as_slice(),
            [HealthInput::FetchOutcome {
                kind: Some(crate::domain::FailureKind::RateLimited),
                ..
            }]
        ));
    }

    #[tokio::test]
    async fn an_unchanged_fetch_clears_a_published_in_flight_summary() {
        let h = Harness::new().await;
        let gate = Gate::new();
        let (id, _) = h.add_stub(|id| StubProvider::new(id).gated(&gate)).await;
        let lease = h.provider_service.acquire(id).await.unwrap();
        gate.open();
        h.fetch_directly(id).await;
        gate.arrived().await;
        gate.close();
        let fetch = h.spawn_fetch(id).await;
        gate.arrived().await;

        h.provider_service
            .publish_summary(id, lease.generation)
            .await;
        gate.open();
        fetch.await.unwrap();

        let last = h
            .bus
            .events()
            .into_iter()
            .rev()
            .find_map(|event| match event {
                CoreEvent::ProviderStatusUpdated { provider } => Some(provider),
                _ => None,
            })
            .unwrap();
        assert!(!last.health.in_flight);
    }

    #[tokio::test]
    async fn a_provider_that_never_answers_times_out_after_thirty_seconds() {
        let h = Harness::new().await;
        let gate = Gate::new();
        let (id, _) = h.add_stub(|id| StubProvider::new(id).gated(&gate)).await;
        h.provider_service.acquire(id).await.unwrap();
        tokio::time::pause();
        let fetch = h.spawn_fetch(id).await;
        gate.arrived().await;
        tokio::time::advance(Duration::from_secs(31)).await;

        while !fetch.is_finished() {
            tokio::task::yield_now().await;
        }
        let result = fetch.await.unwrap();
        let status_read = h.repository.get_provider_fetch_status(id);
        tokio::pin!(status_read);
        let (status, error, _) = loop {
            if let std::task::Poll::Ready(read) = futures::poll!(status_read.as_mut()) {
                break read.unwrap();
            }
            tokio::task::yield_now().await;
        };

        let FetchResult::Committed(Err(DomainError::Timeout(message))) = result else {
            panic!("expected a timeout, got {result:?}");
        };
        assert_eq!(message, "Connection timeout - provider did not respond");
        assert_eq!(status, "error");
        assert!(error.unwrap().contains("did not respond"));
        assert_eq!(
            h.scheduler.view(id, Instant::now()).unwrap().last_failure,
            Some(crate::domain::FailureKind::Timeout)
        );
    }

    #[tokio::test]
    async fn fetch_now_reports_unknown_providers_and_recent_attempts() {
        let h = Harness::new().await;
        h.start().await;
        let (id, _) = h.add_stub(StubProvider::new).await;

        let first = h
            .pipeline_service
            .fetch_now(&[id], Duration::from_secs(5))
            .await;
        let again = h
            .pipeline_service
            .fetch_now(&[id, 4242], Duration::from_secs(5))
            .await;

        assert_eq!(first[0].state, RefreshState::Ok);
        assert_eq!(again[0].state, RefreshState::Recent);
        assert_eq!(again[1].state, RefreshState::NotFound);
    }

    async fn run_history_fixture(h: &Harness, gate: Option<&Arc<Gate>>) -> (i64, String) {
        let (id, _) = h
            .add_stub(|id| {
                let pipeline_id = format!("github__{id}__web");
                let stub = StubProvider::new(id).with_runs(vec![stub_run(&pipeline_id, 1, 60)]);
                match gate {
                    Some(gate) => stub.run_gated(gate),
                    None => stub,
                }
            })
            .await;
        h.fetch_directly(id).await;
        let pipeline_id = format!("github__{id}__web");
        h.metrics
            .as_ref()
            .unwrap()
            .update_pipeline_config(&pipeline_id, true, 30)
            .await
            .unwrap();

        (id, pipeline_id)
    }

    async fn stored_metrics(h: &Harness, pipeline_id: &str) -> usize {
        h.metrics
            .as_ref()
            .unwrap()
            .query_metrics(crate::domain::MetricsQuery {
                pipeline_id: Some(pipeline_id.to_string()),
                ..Default::default()
            })
            .await
            .unwrap()
            .len()
    }

    #[tokio::test]
    async fn fetched_run_history_is_cached_and_produces_metrics() {
        let h = Harness::with_metrics().await;
        let (_, pipeline_id) = run_history_fixture(&h, None).await;

        let page = h
            .pipeline_service
            .fetch_run_history_paginated(&pipeline_id, 1, 20)
            .await
            .unwrap();
        h.pipeline_service.settle().await;

        assert_eq!(page.runs.len(), 1);
        assert_eq!(
            h.repository
                .get_cached_run_count(&pipeline_id)
                .await
                .unwrap(),
            1
        );
        assert!(stored_metrics(&h, &pipeline_id).await > 0);
        assert_eq!(h.bus.count("metrics-generated"), 1);
        assert_eq!(h.bus.count("run-history-cache-invalidated"), 1);
    }

    #[tokio::test]
    async fn cached_run_history_still_produces_metrics() {
        let h = Harness::with_metrics().await;
        let (_, pipeline_id) = run_history_fixture(&h, None).await;
        h.repository
            .cache_run_history(&pipeline_id, &[stub_run(&pipeline_id, 1, 60)])
            .await
            .unwrap();

        let page = h
            .pipeline_service
            .fetch_run_history_paginated(&pipeline_id, 1, 1)
            .await
            .unwrap();
        h.pipeline_service.settle().await;

        assert_eq!(page.runs.len(), 1);
        assert!(stored_metrics(&h, &pipeline_id).await > 0);
        assert_eq!(h.bus.count("metrics-generated"), 1);
    }

    #[tokio::test]
    async fn a_run_history_fetch_finishing_after_a_removal_writes_nothing() {
        let h = Harness::with_metrics().await;
        let gate = Gate::new();
        let (id, pipeline_id) = run_history_fixture(&h, Some(&gate)).await;
        let fetch = tokio::spawn({
            let service = Arc::clone(&h.pipeline_service);
            let pipeline_id = pipeline_id.clone();
            async move {
                service
                    .fetch_run_history_paginated(&pipeline_id, 1, 20)
                    .await
            }
        });
        gate.arrived().await;

        h.provider_service.remove_provider(id).await.unwrap();
        let removed_at = h.bus.events().len();
        gate.open();
        fetch.await.unwrap().unwrap();
        h.pipeline_service.settle().await;

        assert_eq!(
            h.repository
                .get_cached_run_count(&pipeline_id)
                .await
                .unwrap(),
            0
        );
        assert_eq!(stored_metrics(&h, &pipeline_id).await, 0);
        let later = &h.bus.names()[removed_at..];
        assert!(
            !later
                .iter()
                .any(|name| matches!(*name, "metrics-generated" | "run-history-cache-invalidated")),
            "{later:?}"
        );
    }

    #[tokio::test]
    async fn cached_run_history_metrics_after_a_removal_write_nothing() {
        let h = Harness::with_metrics().await;
        let (id, pipeline_id) = run_history_fixture(&h, None).await;
        h.repository
            .cache_run_history(&pipeline_id, &[stub_run(&pipeline_id, 1, 60)])
            .await
            .unwrap();
        let lock = h.provider_service.commit_lock(id);
        let held = lock.lock().await;
        let removal = tokio::spawn({
            let service = Arc::clone(&h.provider_service);
            async move { service.remove_provider(id).await }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        let page = h
            .pipeline_service
            .fetch_run_history_paginated(&pipeline_id, 1, 1)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(held);
        removal.await.unwrap().unwrap();
        h.pipeline_service.settle().await;

        assert_eq!(page.runs.len(), 1);
        assert_eq!(stored_metrics(&h, &pipeline_id).await, 0);
        assert_eq!(h.bus.count("metrics-generated"), 0);
        assert_eq!(
            h.repository
                .get_cached_run_count(&pipeline_id)
                .await
                .unwrap(),
            0
        );
    }
}
