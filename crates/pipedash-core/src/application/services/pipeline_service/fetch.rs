use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::OwnedSemaphorePermit;
use tokio::time::{
    timeout,
    Instant,
};

use super::{
    PipelineService,
    FETCH_TIMEOUT,
    FETCH_TIMEOUT_MESSAGE,
};
use crate::application::fetch_scheduler::{
    ClaimKind,
    FetchClaim,
    FetchResult,
    Outcome,
};
use crate::application::refresh_manager::RefreshMode;
use crate::application::services::health_service::HealthInput;
use crate::application::services::provider_service::ProviderLease;
use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
};
use crate::event::CoreEvent;

struct Applied {
    pipelines: Vec<Pipeline>,
    changed_ids: Vec<String>,
    failed: Vec<Pipeline>,
    finished: Vec<Pipeline>,
}

impl PipelineService {
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
            claim.release();
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

    pub(super) async fn acquire_permit(&self, kind: ClaimKind) -> Option<OwnedSemaphorePermit> {
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
        match &applied {
            Ok(_) => self.write_status(id, true, None).await,
            Err(error) => self.write_status(id, false, Some(error.to_string())).await,
        };

        let (outcome, result) = match applied {
            Ok(applied) => {
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

        let view = claim
            .take()
            .and_then(|claim| claim.complete(Instant::now(), outcome, result.clone()));
        if let Some(view) = view {
            self.health.send(HealthInput::FetchOutcome {
                provider_id: id,
                generation: lease.generation,
                view,
                kind,
            });
            self.provider_service.emit_status(id).await;
        }

        result
    }

    async fn apply_pipelines(
        &self, lease: &ProviderLease, pipelines: Vec<Pipeline>,
    ) -> DomainResult<Applied> {
        let id = lease.id;
        let mut pipelines = self.mark_overdue(pipelines);
        let old = self.repository.get_cached_pipelines(Some(id)).await?;
        self.repository
            .update_pipelines_cache(id, &pipelines)
            .await?;
        self.present_for_event(&mut pipelines).await;

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
                _ => {
                    changed_ids.push(pipeline.id.clone());
                    if !retains_run_history {
                        tracing::debug!(pipeline = %pipeline.name, "Pipeline changed, invalidating run cache");
                        self.invalidate_run_cache(&pipeline.id).await;
                    }
                }
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

        let failed = needing_attention(&old_by_id, &pipelines);
        let finished = pipelines
            .iter()
            .filter(|pipeline| {
                old_by_id.get(pipeline.id.as_str()).is_some_and(|previous| {
                    pipeline.finished_since(&previous.status, previous.last_run)
                })
            })
            .cloned()
            .collect();

        Ok(Applied {
            pipelines,
            changed_ids,
            failed,
            finished,
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
        if !applied.changed_ids.is_empty() {
            self.event_bus
                .emit(CoreEvent::PipelineStatusChanged {
                    pipeline_ids: applied.changed_ids,
                })
                .await;
        }
        if !applied.failed.is_empty() {
            self.event_bus
                .emit(CoreEvent::PipelinesFailed {
                    pipelines: applied.failed,
                })
                .await;
        }
        if !applied.finished.is_empty() {
            self.event_bus
                .emit(CoreEvent::PipelinesFinished {
                    pipelines: applied.finished,
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
}

pub(super) fn failed_outcome(error: &DomainError) -> Outcome {
    Outcome::Failed {
        kind: error.failure_kind(),
        retry_after: error.retry_after(),
    }
}

pub(super) fn needing_attention(old: &HashMap<&str, &Pipeline>, new: &[Pipeline]) -> Vec<Pipeline> {
    new.iter()
        .filter(|pipeline| pipeline.status.needs_attention())
        .filter(|pipeline| {
            old.get(pipeline.id.as_str()).is_some_and(|previous| {
                previous.status != pipeline.status || previous.last_run != pipeline.last_run
            })
        })
        .cloned()
        .collect()
}
