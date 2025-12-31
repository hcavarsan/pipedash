pub mod application;
pub mod domain;
pub mod event;
pub mod infrastructure;
pub mod logging;
pub mod plugins;

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
}

impl CoreContext {
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

        let metrics_service = if metrics_enabled {
            let metrics_repository = Arc::new(MetricsRepository::new(pool.clone()));
            let service = Arc::new(application::MetricsService::new(metrics_repository));

            match service.check_and_repair_corruption().await {
                Ok(repaired) => {
                    if !repaired.is_empty() {
                        tracing::warn!(
                            count = repaired.len(),
                            pipelines = ?repaired,
                            "AUTO-REPAIRED corrupted metrics processing states on startup"
                        );
                    } else {
                        tracing::debug!("No corrupted metrics states found on startup");
                    }
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "Failed to check/repair metrics corruption on startup - metrics may not work correctly"
                    );
                }
            }

            Some(service)
        } else {
            None
        };

        let repository = Arc::new(Repository::new(
            config_backend,
            infrastructure::database::DatabasePool::Sqlite(cache_pool),
            token_store.clone(),
        ));

        let http_client_manager = Arc::new(infrastructure::HttpClientManager::new()?);

        let provider_service = Arc::new(application::ProviderService::new(
            Arc::clone(&repository),
            Arc::clone(&http_client_manager),
            Arc::clone(&event_bus),
        ));
        let pipeline_service = Arc::new(application::PipelineService::new(
            Arc::clone(&repository),
            Arc::clone(&provider_service),
            metrics_service.clone(),
            Arc::clone(&event_bus),
        ));
        let refresh_manager = Arc::new(application::RefreshManager::new(
            Arc::clone(&pipeline_service),
            metrics_service.clone(),
            Arc::clone(&event_bus),
        ));

        Ok(Self {
            event_bus,
            token_store,
            http_client_manager,
            provider_service,
            pipeline_service,
            metrics_service,
            refresh_manager,
        })
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

        let http_client_manager = Arc::new(infrastructure::HttpClientManager::new()?);

        let config = config_state.get().await;
        let metrics_service = if config.general.metrics_enabled {
            let metrics_repository = Arc::new(
                infrastructure::database::MetricsRepository::new_from_pool(cache_pool),
            );
            let service = Arc::new(application::MetricsService::new(metrics_repository));

            match service.check_and_repair_corruption().await {
                Ok(repaired) => {
                    if !repaired.is_empty() {
                        tracing::warn!(
                            count = repaired.len(),
                            pipelines = ?repaired,
                            "AUTO-REPAIRED corrupted metrics processing states on startup"
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        "Failed to check/repair metrics corruption on startup"
                    );
                }
            }

            Some(service)
        } else {
            None
        };

        let provider_service = Arc::new(application::ProviderService::new(
            repository.clone(),
            Arc::clone(&http_client_manager),
            Arc::clone(&event_bus),
        ));
        let pipeline_service = Arc::new(application::PipelineService::new(
            repository.clone(),
            Arc::clone(&provider_service),
            metrics_service.clone(),
            Arc::clone(&event_bus),
        ));
        let refresh_manager = Arc::new(application::RefreshManager::new(
            Arc::clone(&pipeline_service),
            metrics_service.clone(),
            Arc::clone(&event_bus),
        ));

        Ok(Self {
            event_bus,
            token_store,
            http_client_manager,
            provider_service,
            pipeline_service,
            metrics_service,
            refresh_manager,
        })
    }

    pub async fn start_background_tasks(&self) {
        let provider_service = Arc::clone(&self.provider_service);
        let refresh_manager = Arc::clone(&self.refresh_manager);

        tokio::spawn(async move {
            if let Err(e) = provider_service.load_all_providers().await {
                tracing::warn!("Failed to load providers during startup: {}", e);
            }

            refresh_manager.start().await;
        });
    }

    pub async fn shutdown(&self) {
        self.refresh_manager.stop().await;
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
