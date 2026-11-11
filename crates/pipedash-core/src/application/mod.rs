mod config_watcher;
pub mod fetch_scheduler;
mod lifecycle;
pub mod services;
#[cfg(test)]
pub(crate) mod test_support;

use std::sync::Arc;

pub use config_watcher::ConfigWatcher;
pub use fetch_scheduler::{
    ClaimKind,
    FetchResult,
    FetchScheduler,
    ForcedClaim,
    Generation,
    Outcome,
};
use pipedash_plugin_api::PluginRegistry;
pub use services::correlation_service::CorrelationService;
use services::correlation_service::{
    CorrelationDeps,
    EVENT_INTERVAL,
};
pub use services::device_alerts::recording_bus;
pub use services::health_service::{
    refresh_notifications,
    wanted_notifications,
    HealthHandle,
    HealthInput,
    LocalNotification,
    NotificationState,
    ObservedNotification,
    ProviderHealthService,
    ShownNotification,
};
pub use services::log_service::{
    LogExport,
    LogService,
};
pub use services::metrics_service::MetricsService;
pub use services::pipeline_service::{
    validate_watch_selection,
    watch_snapshot,
    PipelineService,
    RunWatch,
    TriggerDraft,
    WatchedRun,
};
pub use services::pipeline_view_service::PipelineViewService;
use services::presenter::Presenter;
use services::provider_service::ProviderDeps;
pub use services::provider_service::ProviderService;
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
    pub pipeline_view_service: Arc<PipelineViewService>,
    pub correlation_service: Arc<CorrelationService>,
}

impl Services {
    pub fn new(
        repository: Arc<Repository>, http_client_manager: Arc<HttpClientManager>,
        metrics_service: Option<Arc<MetricsService>>, event_bus: Arc<dyn EventBus>,
        config_state: Arc<ConfigState>, log_store: Arc<LogStore>,
        plugin_registry: Arc<PluginRegistry>,
    ) -> Self {
        let (health, health_receivers) = HealthHandle::channel();
        let (dispatcher, commands) = RefreshDispatcher::channel();
        let (mode_tx, mode_rx) = watch::channel(RefreshMode::Active);
        config_state.attach_health(health.clone());
        let view_repository = Arc::clone(&repository);
        let view_config_state = Arc::clone(&config_state);
        let view_event_bus = Arc::clone(&event_bus);
        let correlation_repository = Arc::clone(&repository);
        let correlation_config_state = Arc::clone(&config_state);
        let correlation_event_bus = Arc::clone(&event_bus);
        let correlation_http = Arc::clone(&http_client_manager);

        let provider_service = Arc::new(ProviderService::new(
            ProviderDeps {
                repository: Arc::clone(&repository),
                http_client_manager: Arc::clone(&http_client_manager),
                config_state: Arc::clone(&config_state),
                log_store: Arc::clone(&log_store),
                plugin_registry: Arc::clone(&plugin_registry),
            },
            Arc::clone(&event_bus),
            Arc::new(FetchScheduler::default()),
            health.clone(),
            dispatcher,
        ));
        let presenter = Arc::new(Presenter::new(
            plugin_registry,
            Arc::clone(&provider_service),
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
            Arc::clone(&presenter),
        ));
        let pipeline_service = Arc::new(PipelineService::new(
            repository,
            Arc::clone(&provider_service),
            metrics_service.clone(),
            event_bus,
            mode_rx,
            Arc::clone(&presenter),
        ));
        let refresh_manager = Arc::new(RefreshManager::new(
            Arc::clone(&pipeline_service),
            metrics_service,
            mode_tx,
            commands,
        ));
        let pipeline_view_service = Arc::new(PipelineViewService::new(
            view_repository,
            Arc::clone(&provider_service),
            Arc::clone(&pipeline_service),
            view_config_state,
            view_event_bus,
            Arc::clone(&presenter),
        ));
        let correlation_service = CorrelationService::new(CorrelationDeps {
            repository: correlation_repository,
            provider_service: Arc::clone(&provider_service),
            pipeline_service: Arc::clone(&pipeline_service),
            log_service: Arc::clone(&log_service),
            config_state: correlation_config_state,
            event_bus: correlation_event_bus,
            presenter,
            http: correlation_http,
            event_interval: EVENT_INTERVAL,
        });
        pipeline_service.set_observer(correlation_service.clone());
        provider_service.set_applications(correlation_service.clone());

        Self {
            provider_service,
            pipeline_service,
            refresh_manager,
            health,
            health_service,
            log_service,
            pipeline_view_service,
            correlation_service,
        }
    }
}
