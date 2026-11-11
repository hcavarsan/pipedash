pub mod application;
mod context;
pub mod domain;
pub mod event;
pub mod infrastructure;
pub mod logging;
#[cfg(test)]
mod test_env;

use std::sync::{
    Arc,
    Mutex,
    PoisonError,
};

use application::services::device_alerts;
pub use application::{
    LogExport,
    LogService,
};
use domain::PairedDevice;
pub use domain::{
    DomainError,
    DomainResult,
    FetchStatus,
    LogCacheStats,
    LogDocumentMeta,
    LogLinesPage,
    LogSearchQuery,
    LogSearchResult,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    Provider,
    ProviderConfig,
    ProviderSummary,
    RunLogDocument,
    RunLogTree,
    SignInStart,
    SignInStatus,
    TriggerParams,
};
pub use event::{
    CoreEvent,
    EventBus,
    NoOpEventBus,
};
use infrastructure::database::{
    DeviceAlertRepository,
    DevicePushRepository,
    DeviceRepository,
    MetricsRepository,
    PushRepository,
};
pub use infrastructure::{
    MemoryTokenStore,
    StorageBackend,
    TokenStore,
};
use tokio_util::sync::CancellationToken;

type AlertEvents = Arc<Mutex<Option<tokio::sync::mpsc::Receiver<CoreEvent>>>>;

#[derive(Clone)]
pub struct CoreContext {
    pub event_bus: Arc<dyn EventBus>,

    pub token_store: Arc<dyn TokenStore>,

    pub http_client_manager: Arc<infrastructure::HttpClientManager>,

    pub provider_service: Arc<application::ProviderService>,

    pub pipeline_service: Arc<application::PipelineService>,

    pub metrics_service: Option<Arc<application::MetricsService>>,

    pub refresh_manager: Arc<application::RefreshManager>,

    pub health: application::HealthHandle,

    pub health_service: Arc<application::ProviderHealthService>,

    pub config_state: Arc<infrastructure::ConfigState>,

    pub log_service: Arc<application::LogService>,

    pub pipeline_view_service: Arc<application::PipelineViewService>,

    pub correlation_service: Arc<application::CorrelationService>,

    config_watcher: Arc<application::ConfigWatcher>,

    alert_events: AlertEvents,

    alert_stop: CancellationToken,

    recent_notifications: Arc<device_alerts::RecentNotifications>,
}

impl CoreContext {
    pub async fn start_background_tasks(&self) {
        let provider_service = Arc::clone(&self.provider_service);
        let refresh_manager = Arc::clone(&self.refresh_manager);
        let health_service = Arc::clone(&self.health_service);
        let config_watcher = Arc::clone(&self.config_watcher);
        let health = self.health.clone();
        let alert_events = self
            .alert_events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(alert_events) = alert_events {
            let state = device_alerts::seed(&self.pipeline_service, &self.provider_service).await;
            tokio::spawn(device_alerts::record(
                alert_events,
                state,
                self.alert_stop.clone(),
                device_alerts::Recorder {
                    alerts: self.device_alerts(),
                    devices: self.devices(),
                    bus: Arc::clone(&self.event_bus),
                    recent: Arc::clone(&self.recent_notifications),
                },
            ));
        }

        tokio::spawn(async move {
            if let Err(e) = provider_service.load_all_providers().await {
                tracing::warn!("Failed to load providers during startup: {}", e);
            }

            health_service.start().await;
            health.send(application::HealthInput::ProvidersChanged {
                changed: provider_service.health_inputs().await,
            });
            refresh_manager.start().await;
            config_watcher.start().await;
        });
    }

    pub async fn apply_config_file(&self) -> DomainResult<infrastructure::ConfigSyncResult> {
        self.provider_service
            .apply_config_file(&self.config_state)
            .await
    }

    pub async fn factory_reset(&self) -> DomainResult<usize> {
        let providers = self.provider_service.list_providers().await?;
        let providers_removed = providers.len();
        for provider in providers {
            self.provider_service
                .remove_provider_keeping_applications(provider.id)
                .await?;
        }

        for key in self.config_state.load_providers().await?.keys() {
            self.config_state.remove_provider(key).await?;
        }
        for provider_id in self.token_store.get_all_tokens().await?.into_keys() {
            self.token_store.delete_token(provider_id).await?;
        }
        self.pipeline_service.clear_all_caches_atomic().await?;
        self.pipeline_view_service.reset().await?;
        self.correlation_service.reset().await?;
        self.log_service.clear().await?;
        let metrics = MetricsRepository::new_from_pool(
            self.provider_service.repository().cache_pool().clone(),
        );
        metrics.flush_metrics(None, true).await?;
        metrics.reset_all_processing_states().await?;
        self.device_alerts().delete_all().await?;
        self.device_pushes().delete_all().await?;
        PushRepository::new(self.provider_service.repository().cache_pool().clone())
            .delete_all()
            .await?;
        self.revoke_all_devices().await?;
        Ok(providers_removed)
    }

    async fn revoke_all_devices(&self) -> DomainResult<()> {
        let devices = self.devices();
        let paired = devices.list().await?;
        devices.delete_all().await?;
        for device in paired {
            self.event_bus
                .emit(CoreEvent::DeviceRevoked { id: device.id })
                .await;
        }

        Ok(())
    }

    pub fn devices(&self) -> DeviceRepository {
        DeviceRepository::new(self.provider_service.repository().cache_pool().clone())
    }

    pub fn device_alerts(&self) -> DeviceAlertRepository {
        DeviceAlertRepository::new(self.provider_service.repository().cache_pool().clone())
    }

    pub fn notifications_after(&self, after: u64) -> Vec<application::ObservedNotification> {
        self.recent_notifications.after(after)
    }

    pub fn device_pushes(&self) -> DevicePushRepository {
        DevicePushRepository::new(self.provider_service.repository().cache_pool().clone())
    }

    pub async fn pair_device(
        &self, name: &str, platform: &str,
    ) -> DomainResult<(PairedDevice, String)> {
        let (device, token) = PairedDevice::issue(name, platform);
        self.devices().insert(&device).await?;
        self.event_bus
            .emit(CoreEvent::DevicePaired {
                id: device.id.clone(),
                name: device.name.clone(),
            })
            .await;

        Ok((device, token))
    }

    pub async fn revoke_device(&self, id: &str) -> DomainResult<bool> {
        self.device_pushes().delete(id).await?;
        let removed = self.devices().delete(id).await?;
        if removed {
            self.event_bus
                .emit(CoreEvent::DeviceRevoked { id: id.to_string() })
                .await;
        }

        Ok(removed)
    }

    pub async fn shutdown(&self) {
        self.provider_service.drain_reloads().await;
        self.alert_stop.cancel();
        self.pipeline_service.shutdown();
        self.correlation_service.shutdown();
        self.log_service.shutdown().await;
        self.config_watcher.stop().await;
        self.refresh_manager.stop().await;
        self.health_service.stop().await;
    }

    pub async fn warmup_token_store(&self) -> anyhow::Result<()> {
        tracing::info!("Warming up token store (may take 30-60 seconds on first startup)...");

        self.token_store
            .warmup()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to warm up token store: {}", e))?;

        tracing::info!("Token store warmed up successfully - providers can now access tokens");
        Ok(())
    }
}

#[cfg(test)]
mod tests;
