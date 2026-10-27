mod config_watcher;
pub mod fetch_scheduler;
pub mod services;
#[cfg(test)]
pub(crate) mod test_support;

use std::sync::Arc;

pub use config_watcher::ConfigWatcher;
pub use fetch_scheduler::{
    AttemptHandle,
    BackoffPolicy,
    ClaimKind,
    FetchClaim,
    FetchResult,
    FetchScheduler,
    ForcedClaim,
    Generation,
    Outcome,
    ScheduleView,
};
pub use services::health_service::{
    refresh_notifications,
    HealthHandle,
    HealthInput,
    HealthReceivers,
    HealthSnapshot,
    LocalNotification,
    NotificationState,
    ProviderHealthInput,
    ProviderHealthService,
    ProviderHealthState,
};
pub use services::log_service::{
    LogExport,
    LogService,
};
pub use services::metrics_service::MetricsService;
pub use services::pipeline_service::{
    PipelineService,
    RunWatch,
    WatchedRun,
};
pub use services::provider_service::{
    ProviderLease,
    ProviderService,
};
use tokio::sync::watch;

use crate::event::EventBus;
use crate::infrastructure::database::Repository;
use crate::infrastructure::run_logs::LogStore;
use crate::infrastructure::{
    ConfigState,
    HttpClientManager,
};

mod refresh_manager;
pub use refresh_manager::{
    RefreshCommand,
    RefreshDispatcher,
    RefreshManager,
    RefreshMode,
};

pub struct Services {
    pub provider_service: Arc<ProviderService>,
    pub pipeline_service: Arc<PipelineService>,
    pub refresh_manager: Arc<RefreshManager>,
    pub health: HealthHandle,
    pub health_service: Arc<ProviderHealthService>,
    pub log_service: Arc<LogService>,
}

impl Services {
    pub fn new(
        repository: Arc<Repository>, http_client_manager: Arc<HttpClientManager>,
        metrics_service: Option<Arc<MetricsService>>, event_bus: Arc<dyn EventBus>,
        config_state: Arc<ConfigState>, log_store: Arc<LogStore>,
    ) -> Self {
        let (health, health_receivers) = HealthHandle::channel();
        let (dispatcher, commands) = RefreshDispatcher::channel();
        let (mode_tx, mode_rx) = watch::channel(RefreshMode::Active);
        config_state.attach_health(health.clone());

        let provider_service = Arc::new(ProviderService::new(
            Arc::clone(&repository),
            Arc::clone(&http_client_manager),
            Arc::clone(&event_bus),
            Arc::new(FetchScheduler::default()),
            health.clone(),
            dispatcher,
            Arc::clone(&config_state),
            Arc::clone(&log_store),
        ));
        let health_service = Arc::new(ProviderHealthService::new(
            health_receivers,
            Arc::clone(&provider_service),
            http_client_manager,
            Arc::clone(&event_bus),
            config_state,
            mode_tx.subscribe(),
        ));
        let log_service = Arc::new(LogService::new(
            log_store,
            Arc::clone(&provider_service),
            Arc::clone(&repository),
            Arc::clone(&event_bus),
        ));
        let pipeline_service = Arc::new(PipelineService::new(
            repository,
            Arc::clone(&provider_service),
            metrics_service.clone(),
            event_bus,
            mode_rx,
        ));
        let refresh_manager = Arc::new(RefreshManager::new(
            Arc::clone(&pipeline_service),
            metrics_service,
            mode_tx,
            commands,
        ));

        Self {
            provider_service,
            pipeline_service,
            refresh_manager,
            health,
            health_service,
            log_service,
        }
    }
}
