pub mod application;
pub mod domain;
pub mod event;
pub mod infrastructure;
pub mod logging;
pub mod plugins;
#[cfg(test)]
mod test_env;

use std::sync::Arc;

pub use domain::{
    DomainError,
    DomainResult,
    FetchStatus,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    Provider,
    ProviderConfig,
    ProviderSummary,
    TriggerParams,
};
pub use event::{
    CoreEvent,
    EventBus,
    NoOpEventBus,
};
use infrastructure::database::{
    init_database,
    MetricsRepository,
    Repository,
};
pub use infrastructure::{
    ConflictResolution,
    EnvTokenStore,
    LocalStorage,
    MemoryTokenStore,
    StorageBackend,
    SyncConfig,
    SyncDirection,
    SyncManager,
    SyncResult,
    TokenStore,
};

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

    config_watcher: Arc<application::ConfigWatcher>,
}

impl CoreContext {
    fn from_services(
        event_bus: Arc<dyn EventBus>, token_store: Arc<dyn TokenStore>,
        http_client_manager: Arc<infrastructure::HttpClientManager>,
        metrics_service: Option<Arc<application::MetricsService>>,
        config_state: Arc<infrastructure::ConfigState>, services: application::Services,
    ) -> Self {
        Self {
            event_bus,
            token_store,
            http_client_manager,
            config_watcher: Arc::new(application::ConfigWatcher::new(
                Arc::clone(&services.provider_service),
                Arc::clone(&config_state),
            )),
            provider_service: services.provider_service,
            pipeline_service: services.pipeline_service,
            metrics_service,
            refresh_manager: services.refresh_manager,
            health: services.health,
            health_service: services.health_service,
            config_state,
        }
    }

    pub async fn new(
        data_dir: &std::path::Path, event_bus: Arc<dyn EventBus>, token_store: Arc<dyn TokenStore>,
    ) -> anyhow::Result<Self> {
        std::fs::create_dir_all(data_dir)?;

        let config_state =
            infrastructure::ConfigState::initialize(data_dir, token_store.clone()).await?;

        let config = config_state.get().await;
        let db_path = config.db_path();
        let metrics_enabled = config.general.metrics_enabled;

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let pool = init_database(db_path).await?;

        let config_backend = Arc::new(infrastructure::SqliteConfigBackend::new(pool.clone()))
            as Arc<dyn infrastructure::ConfigBackend>;
        let cache_pool = pool.clone();

        let sync_result = config_state
            .sync_providers_from_toml(config_backend.as_ref(), false)
            .await?;
        if !sync_result.added.is_empty()
            || !sync_result.updated.is_empty()
            || !sync_result.removed.is_empty()
        {
            tracing::info!(
                added = sync_result.added.len(),
                updated = sync_result.updated.len(),
                removed = sync_result.removed.len(),
                "Synced providers from TOML to database on startup"
            );
        }

        let metrics_service = metrics_enabled.then(|| {
            let metrics_repository = Arc::new(MetricsRepository::new(pool.clone()));
            Arc::new(application::MetricsService::new(metrics_repository))
        });

        let repository = Arc::new(Repository::new(
            config_backend,
            infrastructure::database::DatabasePool::Sqlite(cache_pool),
            token_store.clone(),
        ));

        let http_client_manager = http_client_manager(&config.general.proxy_url)?;

        let services = application::Services::new(
            repository,
            Arc::clone(&http_client_manager),
            metrics_service.clone(),
            Arc::clone(&event_bus),
            Arc::clone(&config_state),
        );

        Ok(Self::from_services(
            event_bus,
            token_store,
            http_client_manager,
            metrics_service,
            config_state,
            services,
        ))
    }

    pub async fn with_storage_manager(
        storage_manager: &infrastructure::StorageManager, event_bus: Arc<dyn EventBus>,
    ) -> anyhow::Result<Self> {
        let token_store = storage_manager.token_store().await;
        let config_backend = storage_manager.config_backend();
        let data_dir = storage_manager.config().data_dir();
        let _storage_config = storage_manager.config();

        let config_state =
            infrastructure::ConfigState::initialize(&data_dir, token_store.clone()).await?;

        let sync_result = config_state
            .sync_providers_from_toml(config_backend.as_ref(), false)
            .await?;
        if !sync_result.added.is_empty()
            || !sync_result.updated.is_empty()
            || !sync_result.removed.is_empty()
        {
            tracing::info!(
                added = sync_result.added.len(),
                updated = sync_result.updated.len(),
                removed = sync_result.removed.len(),
                "Synced providers from TOML to database on startup"
            );
        }

        let cache_pool = storage_manager.database_pool();

        let repository = Arc::new(infrastructure::database::Repository::new(
            config_backend.clone(),
            cache_pool.clone(),
            token_store.clone(),
        ));

        let config = config_state.get().await;
        let http_client_manager = http_client_manager(&config.general.proxy_url)?;
        let metrics_service = config.general.metrics_enabled.then(|| {
            let metrics_repository = Arc::new(
                infrastructure::database::MetricsRepository::new_from_pool(cache_pool),
            );
            Arc::new(application::MetricsService::new(metrics_repository))
        });

        let services = application::Services::new(
            repository,
            Arc::clone(&http_client_manager),
            metrics_service.clone(),
            Arc::clone(&event_bus),
            Arc::clone(&config_state),
        );

        Ok(Self::from_services(
            event_bus,
            token_store,
            http_client_manager,
            metrics_service,
            config_state,
            services,
        ))
    }

    pub async fn start_background_tasks(&self) {
        let provider_service = Arc::clone(&self.provider_service);
        let refresh_manager = Arc::clone(&self.refresh_manager);
        let health_service = Arc::clone(&self.health_service);
        let config_watcher = Arc::clone(&self.config_watcher);
        let health = self.health.clone();

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

    pub async fn shutdown(&self) {
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

fn http_client_manager(proxy_url: &str) -> DomainResult<Arc<infrastructure::HttpClientManager>> {
    let manager = infrastructure::HttpClientManager::new()?;
    if let Err(e) = manager.set_proxy(proxy_url) {
        tracing::error!(error = %e, "Ignoring general.proxy_url from config.toml");
    }

    Ok(Arc::new(manager))
}
