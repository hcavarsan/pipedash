use std::sync::{
    Arc,
    PoisonError,
};

use crate::application::services::correlation_service::transitions::health_of;
use crate::application::services::correlation_service::{
    CorrelationService,
    HISTORY_BACKFILL_LIMIT,
};
use crate::domain::{
    DomainResult,
    Pipeline,
};
use crate::infrastructure::database::CorrelationConn;

#[derive(Default)]
pub(super) struct Followed {
    pub stale: Vec<String>,
    pub shown: bool,
}

impl CorrelationService {
    pub(super) async fn follow(
        &self, conn: &mut CorrelationConn, followed: &mut Followed, old: &Pipeline, new: &Pipeline,
    ) -> DomainResult<()> {
        let stale = old.status != new.status || old.last_run != new.last_run;
        let shown = old.status != new.status || health_of(old) != health_of(new);
        if !(stale || shown)
            || conn
                .list_application_members_by_pipeline(&new.id)
                .await?
                .is_empty()
        {
            return Ok(());
        }
        followed.shown |= shown;
        if stale {
            followed.stale.push(new.id.clone());
        }
        Ok(())
    }

    pub(in crate::application::services::correlation_service) fn refetch_history(
        &self, pipelines: impl IntoIterator<Item = String>,
    ) {
        let Some(this) = self.arc() else {
            return;
        };
        for pipeline in pipelines {
            if !self.claim_refetch(&pipeline) {
                continue;
            }
            let this = Arc::clone(&this);
            self.spawn(async move { this.refetch_loop(pipeline).await });
        }
    }

    async fn refetch_loop(&self, pipeline: String) {
        loop {
            let Ok(permit) = Arc::clone(&self.enrich_permits).acquire_owned().await else {
                self.refetching
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&pipeline);
                return;
            };
            if let Err(error) = self
                .pipeline_service
                .fetch_run_history(&pipeline, HISTORY_BACKFILL_LIMIT)
                .await
            {
                tracing::warn!(%error, pipeline, "Run history refresh failed");
            }
            drop(permit);
            if !self.release_refetch(&pipeline) {
                return;
            }
        }
    }

    fn claim_refetch(&self, pipeline: &str) -> bool {
        let mut inflight = self
            .refetching
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        match inflight.get_mut(pipeline) {
            Some(again) => {
                *again = true;
                false
            }
            None => {
                inflight.insert(pipeline.to_string(), false);
                true
            }
        }
    }

    fn release_refetch(&self, pipeline: &str) -> bool {
        let mut inflight = self
            .refetching
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let again = inflight.get_mut(pipeline).map(std::mem::take);
        if again != Some(true) {
            inflight.remove(pipeline);
        }
        again == Some(true)
    }
}
