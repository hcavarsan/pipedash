use std::sync::{
    Arc,
    Weak,
};
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use tokio::time::{
    sleep,
    sleep_until,
    timeout,
    Instant,
};

use super::fetch::failed_outcome;
use super::{
    PipelineService,
    FETCH_TIMEOUT_MESSAGE,
};
use crate::application::fetch_scheduler::{
    FetchClaim,
    FetchResult,
    ForcedClaim,
    Generation,
};
use crate::application::{
    ClaimKind,
    HealthInput,
    Outcome,
};
use crate::domain::{
    DomainError,
    DomainResult,
    FailureKind,
    Pipeline,
    PipelineRun,
    PipelineStatus,
};

const RUN_WATCH_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct WatchedRun {
    pub pipeline: Pipeline,
    pub provider_name: String,
    pub run: PipelineRun,
    pub observed_at: DateTime<Utc>,
}

pub struct RunWatch {
    service: Weak<PipelineService>,
    pipeline_id: String,
    provider_id: i64,
    generation: Generation,
    run_number: i64,
    next_attempt: Instant,
    finished: bool,
}

struct RunAttempt(Option<FetchClaim>);

impl Drop for RunAttempt {
    fn drop(&mut self) {
        if let Some(claim) = self.0.take() {
            claim.release();
        }
    }
}

impl PipelineService {
    pub async fn watch_run(
        self: &Arc<Self>, pipeline_id: String, run_number: i64,
    ) -> DomainResult<RunWatch> {
        if run_number <= 0 {
            return Err(DomainError::InvalidConfig(
                "Choose a positive run number".to_string(),
            ));
        }
        let pipeline = self.repository.get_cached_pipeline(&pipeline_id).await?;
        let lease = self.provider_service.acquire(pipeline.provider_id).await?;
        Ok(RunWatch {
            service: Arc::downgrade(self),
            pipeline_id,
            provider_id: pipeline.provider_id,
            generation: lease.generation,
            run_number,
            next_attempt: Instant::now(),
            finished: false,
        })
    }
}

impl RunWatch {
    pub fn provider_id(&self) -> i64 {
        self.provider_id
    }

    pub async fn next(&mut self) -> Option<DomainResult<WatchedRun>> {
        if self.finished {
            return None;
        }
        sleep_until(self.next_attempt).await;
        let result = match self.service.upgrade() {
            Some(service) => timeout(PipelineService::REFRESH_DEADLINE, self.observe(&service))
                .await
                .unwrap_or_else(|_| Err(DomainError::Timeout(FETCH_TIMEOUT_MESSAGE.to_string()))),
            None => Err(superseded()),
        };
        self.finished = match &result {
            Ok(observed) => !matches!(
                observed.run.status,
                PipelineStatus::Pending | PipelineStatus::Running
            ),
            Err(_) => true,
        };
        self.next_attempt = Instant::now() + RUN_WATCH_INTERVAL;
        Some(result)
    }

    async fn observe(&self, service: &PipelineService) -> DomainResult<WatchedRun> {
        let _priority = service
            .scheduler
            .prioritize_forced(self.provider_id, self.generation)
            .ok_or_else(superseded)?;
        loop {
            let lease = service.provider_service.acquire(self.provider_id).await?;
            if lease.generation != self.generation {
                return Err(superseded());
            }
            if let Some(until) = service.scheduler.paused_until(self.provider_id, Utc::now()) {
                return Err(rate_limited((until - Utc::now()).to_std().ok()));
            }
            let claim = match service.scheduler.claim_forced(
                self.provider_id,
                self.generation,
                Instant::now(),
            ) {
                ForcedClaim::Claimed(claim) => claim,
                ForcedClaim::Joined(handle) => {
                    drop(lease);
                    handle.wait().await;
                    continue;
                }
                ForcedClaim::Recent(Outcome::Failed {
                    kind: FailureKind::RateLimited,
                    ..
                }) => {
                    let retry_after = service
                        .scheduler
                        .forced_retry_in(self.provider_id, self.generation, Instant::now())
                        .ok_or_else(superseded)?;
                    return Err(rate_limited(Some(retry_after)));
                }
                ForcedClaim::Recent(_) => {
                    let wait = service
                        .scheduler
                        .forced_retry_in(self.provider_id, self.generation, Instant::now())
                        .ok_or_else(superseded)?;
                    drop(lease);
                    sleep(wait).await;
                    continue;
                }
                ForcedClaim::Refused => return Err(superseded()),
            };
            let mut attempt = RunAttempt(Some(claim));
            let permit = service
                .acquire_permit(ClaimKind::Forced)
                .await
                .ok_or_else(superseded)?;
            if service
                .provider_service
                .commit(&lease, || async {})
                .await
                .is_none()
            {
                return Err(superseded());
            }
            if let Some(until) = service.provider_service.check_quota(&lease).await {
                return Err(rate_limited((until - Utc::now()).to_std().ok()));
            }
            let pipeline = self.cached_pipeline(service).await?;
            if service
                .provider_service
                .commit(&lease, || async {})
                .await
                .is_none()
            {
                return Err(superseded());
            }
            let fetched = service
                .fetch_run_details_from(lease.provider.as_ref(), &pipeline, self.run_number)
                .await;
            drop(permit);
            return service
                .provider_service
                .commit(&lease, || async {
                    let run = match fetched {
                        Ok(run) => run,
                        Err(error) => {
                            self.record_failure(service, &mut attempt, &error).await;
                            return Err(error);
                        }
                    };
                    let pipeline = self.cached_pipeline(service).await?;
                    let config = service.repository.get_provider(self.provider_id).await?;
                    if run.pipeline_id != self.pipeline_id
                        || run.run_number != self.run_number
                        || config.id != Some(self.provider_id)
                        || config.provider_type != pipeline.provider_type
                    {
                        return Err(DomainError::InvalidConfig(
                            "The provider returned a different pipeline or build run".to_string(),
                        ));
                    }
                    let (mut pipeline, mut run) = (pipeline, run);
                    let presenter = &service.presenter;
                    presenter
                        .pipelines(std::slice::from_mut(&mut pipeline))
                        .await?;
                    presenter
                        .runs(&pipeline, std::slice::from_mut(&mut run))
                        .await?;
                    Ok(WatchedRun {
                        pipeline,
                        provider_name: config.display_name().to_string(),
                        run,
                        observed_at: Utc::now(),
                    })
                })
                .await
                .ok_or_else(superseded)?;
        }
    }

    async fn cached_pipeline(&self, service: &PipelineService) -> DomainResult<Pipeline> {
        let pipeline = service
            .repository
            .get_cached_pipeline(&self.pipeline_id)
            .await?;
        if pipeline.provider_id != self.provider_id {
            return Err(DomainError::PipelineNotFound(self.pipeline_id.clone()));
        }

        Ok(pipeline)
    }

    async fn record_failure(
        &self, service: &PipelineService, attempt: &mut RunAttempt, error: &DomainError,
    ) {
        let Some(view) = attempt.0.take().and_then(|claim| {
            claim.complete(
                Instant::now(),
                failed_outcome(error),
                FetchResult::Cancelled,
            )
        }) else {
            return;
        };
        service.health.send(HealthInput::FetchOutcome {
            provider_id: self.provider_id,
            generation: self.generation,
            view,
            kind: Some(error.failure_kind()),
        });
        service.provider_service.emit_status(self.provider_id).await;
    }
}

fn superseded() -> DomainError {
    DomainError::ConcurrentModification(
        "The watched provider or application session changed".to_string(),
    )
}

fn rate_limited(retry_after: Option<Duration>) -> DomainError {
    DomainError::RateLimited {
        message: "Provider API quota or rate-limit cooldown is active".to_string(),
        retry_after,
    }
}

#[cfg(test)]
mod tests;
