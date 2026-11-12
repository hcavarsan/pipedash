use std::collections::HashSet;
use std::sync::{
    Arc,
    Mutex as StdMutex,
};
use std::time::Duration;

use dashmap::DashMap;
use futures::future::{
    BoxFuture,
    Shared,
};
use tokio::sync::{
    watch,
    Mutex,
    Semaphore,
};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::application::fetch_scheduler::FetchScheduler;
use crate::application::refresh_manager::{
    RefreshDispatcher,
    RefreshMode,
};
use crate::application::services::health_service::HealthHandle;
use crate::application::services::metrics_service::MetricsService;
use crate::application::services::presenter::{
    Presenter,
    READ_ONLY_PROVIDER,
};
use crate::application::services::provider_service::ProviderService;
use crate::domain::{
    DomainError,
    DomainResult,
    PipelineRun,
    RefreshResult,
};
use crate::event::EventBus;
use crate::infrastructure::database::Repository;
use crate::infrastructure::deduplication::RequestDeduplicator;

mod actions;
mod background;
mod cache;
mod cancellation;
mod fetch;
mod history;
pub(crate) mod overdue;
mod refresh;
mod run_watch;
#[cfg(test)]
mod tests;
pub(crate) mod timing;
mod trigger;
mod watch_view;

pub use run_watch::{
    RunWatch,
    WatchedRun,
};
pub use trigger::TriggerDraft;
pub use watch_view::{
    validate_watch_selection,
    watch_snapshot,
};

pub(crate) const FETCH_CONCURRENCY: usize = 10;
const METRICS_CONCURRENCY: usize = 4;
pub(crate) const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const FETCH_TIMEOUT_MESSAGE: &str = "Connection timeout - provider did not respond";
const FORCE_FEEDS_MIN_AGE: Duration = Duration::from_secs(15);
pub(crate) const OVERDUE_RECENT_RUNS: usize = 20;
pub(crate) const OVERDUE_CONCURRENT_CHECKS: usize = 4;

type RefreshFlight = Shared<BoxFuture<'static, DomainResult<Vec<RefreshResult>>>>;

#[derive(Clone, PartialEq)]
struct RunTiming {
    typical: i64,
    overdue: Option<String>,
}

pub struct PipelineService {
    repository: Arc<Repository>,
    provider_service: Arc<ProviderService>,
    metrics_service: Option<Arc<MetricsService>>,
    event_bus: Arc<dyn EventBus>,
    run_deduplicator: Arc<RequestDeduplicator<Vec<PipelineRun>, DomainError>>,
    run_timings: Arc<DashMap<String, RunTiming>>,
    scheduler: Arc<FetchScheduler>,
    fetch_permits: Arc<Semaphore>,
    health: HealthHandle,
    mode: watch::Receiver<RefreshMode>,
    dispatcher: RefreshDispatcher,
    refresh_flight: Mutex<Option<RefreshFlight>>,
    background: StdMutex<JoinSet<()>>,
    root: CancellationToken,
    metrics_inflight: Arc<StdMutex<HashSet<String>>>,
    overdue_inflight: Arc<StdMutex<HashSet<String>>>,
    metrics_permits: Arc<Semaphore>,
    presenter: Arc<Presenter>,
}

impl PipelineService {
    pub const REFRESH_DEADLINE: Duration = Duration::from_secs(25);

    pub(crate) fn new(
        repository: Arc<Repository>, provider_service: Arc<ProviderService>,
        metrics_service: Option<Arc<MetricsService>>, event_bus: Arc<dyn EventBus>,
        mode: watch::Receiver<RefreshMode>, presenter: Arc<Presenter>,
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
            run_timings: Arc::new(DashMap::new()),
            fetch_permits: Arc::new(Semaphore::new(FETCH_CONCURRENCY)),
            mode,
            refresh_flight: Mutex::new(None),
            background: StdMutex::new(JoinSet::new()),
            root: CancellationToken::new(),
            metrics_inflight: Arc::new(StdMutex::new(HashSet::new())),
            overdue_inflight: Arc::new(StdMutex::new(HashSet::new())),
            metrics_permits: Arc::new(Semaphore::new(METRICS_CONCURRENCY)),
            presenter,
        }
    }

    async fn present_for_event(&self, pipelines: &mut [crate::domain::Pipeline]) {
        if let Err(error) = self.presenter.pipelines(pipelines).await {
            tracing::warn!(%error, "Failed to present pipelines for an event");
        }
    }

    pub fn shutdown(&self) {
        self.root.cancel();
    }
}
