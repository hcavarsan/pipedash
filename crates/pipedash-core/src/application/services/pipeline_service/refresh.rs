use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use futures::future::FutureExt;
use tokio::time::{
    timeout_at,
    Instant,
};

use super::{
    PipelineService,
    RefreshFlight,
    FORCE_FEEDS_MIN_AGE,
};
use crate::application::fetch_scheduler::{
    FetchResult,
    ForcedClaim,
    Outcome,
};
use crate::application::services::health_service::HealthInput;
use crate::application::services::provider_service::ProviderLease;
use crate::domain::{
    DomainError,
    DomainResult,
    FailureKind,
    RefreshResult,
    RefreshState,
};
use crate::event::CoreEvent;

impl PipelineService {
    pub(super) async fn refresh_overdue(&self, lease: &ProviderLease) {
        if !self.update_overdue_pipelines(lease.id).await {
            return;
        }

        self.provider_service
            .commit(lease, || async {
                let pipelines = match self.get_cached_pipelines(Some(lease.id)).await {
                    Ok(pipelines) => pipelines,
                    Err(e) => {
                        tracing::warn!(provider_id = lease.id, error = %e, "Failed to publish overdue pipelines");
                        return;
                    }
                };
                self.event_bus
                    .emit(CoreEvent::PipelinesUpdated {
                        pipelines,
                        provider_id: lease.id,
                        timestamp: Utc::now().timestamp_millis(),
                    })
                    .await;
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
            let (handle, joined) =
                match self
                    .scheduler
                    .claim_forced(id, lease.generation, Instant::now())
                {
                    ForcedClaim::Claimed(claim) => {
                        let handle = claim.handle();
                        self.dispatcher.fetch_now(vec![claim]);
                        (handle, false)
                    }
                    ForcedClaim::Joined(handle) => (handle, true),
                    ForcedClaim::Recent(outcome) => return recent_result(id, outcome),
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
                FetchResult::Cancelled if joined => continue,
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
                    let task = tokio::spawn(async move { service.refresh_until_shutdown().await });
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

    async fn refresh_until_shutdown(&self) -> DomainResult<Vec<RefreshResult>> {
        tokio::select! {
            result = self.run_refresh_all() => result,
            () = self.root.cancelled() => Err(DomainError::InternalError(
                "Refresh cancelled: the application is shutting down".to_string(),
            )),
        }
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
}

fn recent_result(provider_id: i64, outcome: Outcome) -> RefreshResult {
    RefreshResult {
        provider_id,
        state: RefreshState::Recent,
        kind: match outcome {
            Outcome::Failed { kind, .. } => Some(kind),
            Outcome::Success | Outcome::Cancelled => None,
        },
        message: None,
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
