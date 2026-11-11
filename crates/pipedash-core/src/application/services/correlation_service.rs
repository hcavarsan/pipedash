mod applications;
mod changes;
mod commit_queries;
mod config;
mod configured;
mod copy;
mod edges;
mod enrich;
mod hubs;
mod membership;
mod metrics;
mod names;
mod observe;
mod overrides;
mod page;
mod resolve;
mod state;
mod stored;
mod suggestions;
mod tags;
mod targets;
#[cfg(test)]
mod tests;
mod toml;
mod transitions;
mod vocab;
mod world;

use std::future::Future;
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex as StdMutex,
    PoisonError,
    Weak,
};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{
    Mutex,
    RwLock,
    Semaphore,
};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::pipeline_service::observer::FetchObserver;
use super::pipeline_service::PipelineService;
use super::presenter::Presenter;
use super::provider_service::ProviderService;
use crate::application::services::log_service::LogService;
use crate::domain::application::dto::ConfigLoadError;
use crate::domain::{
    DomainResult,
    Pipeline,
    PipelineRun,
};
use crate::event::{
    CoreEvent,
    EventBus,
};
use crate::infrastructure::database::{
    CorrelationRepository,
    Repository,
};
use crate::infrastructure::{
    ConfigState,
    HttpClientManager,
};

pub const HUB_UNIT_LIMIT: usize = 5;
pub const SUMMARY_CHIP_LIMIT: usize = 6;
pub const LOG_SCAN_HOURS: i64 = 24;
pub const LOG_SCAN_RUNS: usize = 50;
pub const FAILURE_WINDOW_MINUTES: i64 = 60;
pub const OBSERVATION_GAP_FACTOR: i64 = 3;
pub const OBSERVATION_GAP_FLOOR_SECS: i64 = 120;
pub const HISTORY_BACKFILL_LIMIT: usize = 100;
pub const ENRICH_PERMITS: usize = 4;
pub const EVENT_INTERVAL: Duration = Duration::from_millis(500);

pub(crate) struct CorrelationDeps {
    pub repository: Arc<Repository>,
    pub provider_service: Arc<ProviderService>,
    pub pipeline_service: Arc<PipelineService>,
    pub log_service: Arc<LogService>,
    pub config_state: Arc<ConfigState>,
    pub event_bus: Arc<dyn EventBus>,
    pub presenter: Arc<Presenter>,
    pub http: Arc<HttpClientManager>,
    pub event_interval: Duration,
}

struct Emitter {
    dirty: Arc<AtomicBool>,
    pumping: Arc<AtomicBool>,
    interval: Duration,
}

pub struct CorrelationService {
    me: Weak<CorrelationService>,
    enrich: enrich::EnrichState,
    store: CorrelationRepository,
    repository: Arc<Repository>,
    provider_service: Arc<ProviderService>,
    pipeline_service: Arc<PipelineService>,
    log_service: Arc<LogService>,
    config_state: Arc<ConfigState>,
    event_bus: Arc<dyn EventBus>,
    presenter: Arc<Presenter>,
    http: Arc<HttpClientManager>,
    write_lock: Mutex<()>,
    config_error: RwLock<Option<ConfigLoadError>>,
    background: StdMutex<JoinSet<()>>,
    cancel: CancellationToken,
    enrich_permits: Arc<Semaphore>,
    emitter: Emitter,
}

impl CorrelationService {
    pub(crate) fn new(deps: CorrelationDeps) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            enrich: enrich::EnrichState::default(),
            store: CorrelationRepository::new(deps.repository.cache_pool().clone()),
            repository: deps.repository,
            provider_service: deps.provider_service,
            pipeline_service: deps.pipeline_service,
            log_service: deps.log_service,
            config_state: deps.config_state,
            event_bus: deps.event_bus,
            presenter: deps.presenter,
            http: deps.http,
            write_lock: Mutex::new(()),
            config_error: RwLock::new(None),
            background: StdMutex::new(JoinSet::new()),
            cancel: CancellationToken::new(),
            enrich_permits: Arc::new(Semaphore::new(ENRICH_PERMITS)),
            emitter: Emitter {
                dirty: Arc::new(AtomicBool::new(false)),
                pumping: Arc::new(AtomicBool::new(false)),
                interval: deps.event_interval,
            },
        })
    }

    pub(super) fn arc(&self) -> Option<Arc<Self>> {
        self.me.upgrade()
    }

    pub async fn config_error(&self) -> Option<ConfigLoadError> {
        self.config_error.read().await.clone()
    }

    pub(super) fn spawn(&self, task: impl Future<Output = ()> + Send + 'static) {
        let cancel = self.cancel.clone();
        let mut background = self
            .background
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        while background.try_join_next().is_some() {}
        background.spawn(async move {
            tokio::select! {
                _ = cancel.cancelled() => {}
                _ = task => {}
            }
        });
    }

    pub(super) fn changed(&self) {
        self.emitter.dirty.store(true, Ordering::SeqCst);
        if self.emitter.pumping.swap(true, Ordering::SeqCst) {
            return;
        }
        let dirty = Arc::clone(&self.emitter.dirty);
        let pumping = Arc::clone(&self.emitter.pumping);
        let bus = Arc::clone(&self.event_bus);
        let interval = self.emitter.interval;
        self.spawn(async move {
            tokio::time::sleep(interval).await;
            pumping.store(false, Ordering::SeqCst);
            if dirty.swap(false, Ordering::SeqCst) {
                bus.emit(CoreEvent::ApplicationsChanged).await;
            }
        });
    }

    pub(super) async fn flush_events(&self) {
        if self.emitter.dirty.swap(false, Ordering::SeqCst) {
            self.event_bus.emit(CoreEvent::ApplicationsChanged).await;
        }
    }

    pub async fn reset(&self) -> DomainResult<()> {
        let _guard = self.write_lock.lock().await;
        self.store.conn().clear().await?;
        *self.config_error.write().await = None;
        self.changed();
        self.flush_events().await;
        Ok(())
    }

    pub fn shutdown(&self) {
        self.cancel.cancel();
    }
}

#[async_trait]
impl FetchObserver for CorrelationService {
    async fn pipelines_applied(&self, provider_id: i64, old: &[Pipeline], new: &[Pipeline]) {
        if let Err(error) = self.pipelines_applied_impl(provider_id, old, new).await {
            tracing::warn!(%error, provider_id, "Correlation could not observe a refresh");
        }
    }

    async fn runs_fetched(&self, provider_id: i64, pipeline_id: &str, runs: &[PipelineRun]) {
        if let Err(error) = self.runs_fetched_impl(provider_id, pipeline_id, runs).await {
            tracing::warn!(%error, pipeline_id, "Correlation could not observe run history");
        }
    }
}
