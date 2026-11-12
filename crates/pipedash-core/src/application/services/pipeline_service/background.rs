use std::collections::HashSet;
use std::future::Future;
use std::sync::{
    Arc,
    Mutex as StdMutex,
    PoisonError,
};

use pipedash_plugin_api::RunHistoryOrder;

use super::PipelineService;
use crate::application::services::metrics_service::MetricsService;
use crate::application::services::provider_service::ProviderLease;
use crate::domain::{
    DomainResult,
    PipelineRun,
};
use crate::event::{
    CoreEvent,
    EventBus,
};
use crate::infrastructure::database::Repository;

impl PipelineService {
    pub(super) fn spawn_metrics(
        &self, lease: ProviderLease, pipeline_id: String, runs: Option<Vec<PipelineRun>>,
    ) {
        let Some(metrics_service) = self.metrics_service.clone() else {
            return;
        };
        let Some(guard) = InflightGuard::claim(&self.metrics_inflight, pipeline_id.clone()) else {
            return;
        };
        let permits = Arc::clone(&self.metrics_permits);
        let provider_service = Arc::clone(&self.provider_service);
        let repository = Arc::clone(&self.repository);
        let event_bus = Arc::clone(&self.event_bus);

        let order = lease.provider.run_history_policy().order;
        self.spawn_background(async move {
            let _guard = guard;
            let Ok(_permit) = permits.acquire_owned().await else {
                return;
            };
            provider_service
                .commit(&lease, || async {
                    let generated = generate_metrics(
                        &repository,
                        &metrics_service,
                        event_bus.as_ref(),
                        &pipeline_id,
                        runs,
                        order,
                    )
                    .await;
                    if let Err(e) = generated {
                        tracing::error!(
                            error = %e,
                            pipeline_id = %pipeline_id,
                            "Failed to extract and store metrics"
                        );
                    }
                })
                .await;
        });
    }

    pub(super) fn spawn_background(&self, task: impl Future<Output = ()> + Send + 'static) {
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
}

pub(super) struct InflightGuard {
    inflight: Arc<StdMutex<HashSet<String>>>,
    key: String,
}

impl InflightGuard {
    pub(super) fn claim(inflight: &Arc<StdMutex<HashSet<String>>>, key: String) -> Option<Self> {
        if !inflight
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.clone())
        {
            return None;
        }
        Some(Self {
            inflight: Arc::clone(inflight),
            key,
        })
    }
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        self.inflight
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.key);
    }
}

async fn generate_metrics(
    repository: &Repository, metrics: &MetricsService, event_bus: &dyn EventBus, pipeline_id: &str,
    runs: Option<Vec<PipelineRun>>, order: RunHistoryOrder,
) -> DomainResult<()> {
    let runs = match runs {
        Some(runs) => runs,
        None => {
            repository
                .get_cached_run_history(pipeline_id, 10000, order)
                .await?
        }
    };
    if metrics
        .extract_and_store_metrics(pipeline_id, &runs)
        .await?
        == 0
    {
        return Ok(());
    }
    event_bus
        .emit(CoreEvent::MetricsGenerated {
            pipeline_id: pipeline_id.to_string(),
        })
        .await;

    Ok(())
}
