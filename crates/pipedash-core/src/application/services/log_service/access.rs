use std::future::Future;

use chrono::Utc;

use super::LogService;
use crate::application::services::provider_service::ProviderLease;
use crate::domain::{
    DomainError,
    DomainResult,
    PipelineRun,
};
use crate::infrastructure::run_logs::{
    CacheEpoch,
    DocKey,
};

impl LogService {
    pub(super) async fn provider_of(&self, pipeline_id: &str) -> DomainResult<i64> {
        if let Some(provider_id) = self.pipeline_providers.get(pipeline_id).map(|id| *id) {
            return Ok(provider_id);
        }
        let provider_id = self
            .repository
            .get_cached_pipeline(pipeline_id)
            .await?
            .provider_id;
        self.pipeline_providers
            .insert(pipeline_id.to_string(), provider_id);

        Ok(provider_id)
    }

    pub(super) async fn lease_of(
        &self, pipeline_id: &str,
    ) -> DomainResult<(ProviderLease, CacheEpoch)> {
        let provider_id = self.provider_of(pipeline_id).await?;
        let lease = match self.provider_service.acquire(provider_id).await {
            Err(DomainError::ProviderNotFound(_)) => {
                self.pipeline_providers.remove(pipeline_id);
                return Err(DomainError::PipelineNotFound(pipeline_id.to_string()));
            }
            other => other?,
        };
        let epoch = self.store.epoch(provider_id);
        if !lease.provider.capabilities().logs {
            return Err(DomainError::NotSupported(
                "Run logs are not available for this provider".to_string(),
            ));
        }

        Ok((lease, epoch))
    }

    pub(super) async fn doc_key(
        &self, pipeline_id: &str, run_number: i64, node_id: &str,
    ) -> DomainResult<DocKey> {
        Ok(DocKey {
            provider_id: self.provider_of(pipeline_id).await?,
            pipeline_id: pipeline_id.to_string(),
            run_number,
            node_id: node_id.to_string(),
        })
    }

    pub(super) fn ensure_active(&self, provider_id: i64) -> DomainResult<()> {
        let now = Utc::now();
        match self
            .provider_service
            .scheduler()
            .paused_until(provider_id, now)
        {
            Some(until) => Err(DomainError::RateLimited {
                message: "Provider API quota or rate-limit cooldown is active".to_string(),
                retry_after: (until - now).to_std().ok(),
            }),
            None => Ok(()),
        }
    }

    pub(super) async fn note_limit(&self, lease: &ProviderLease, error: &DomainError) {
        if matches!(
            error,
            DomainError::RateLimited { .. } | DomainError::ServiceUnavailable { .. }
        ) {
            self.provider_service.check_quota(lease).await;
        }
    }

    pub(super) async fn present_run(&self, run: &PipelineRun) -> PipelineRun {
        let mut presented = run.clone();
        let result = async {
            let pipeline = self
                .repository
                .get_cached_pipeline(&run.pipeline_id)
                .await?;
            self.presenter
                .runs(&pipeline, std::slice::from_mut(&mut presented))
                .await
        };
        if let Err(error) = result.await {
            tracing::warn!(pipeline_id = %run.pipeline_id, %error, "Failed to present a run for an event");
        }
        presented
    }

    pub(super) async fn provider_call<T>(
        &self, lease: &ProviderLease, call: impl Future<Output = DomainResult<T>>,
    ) -> DomainResult<T> {
        self.ensure_active(lease.id)?;
        let result = tokio::select! {
            result = call => result,
            _ = self.root.cancelled() => return Err(super::shutting_down()),
        };
        if let Err(e) = &result {
            self.note_limit(lease, e).await;
        }

        result
    }
}
