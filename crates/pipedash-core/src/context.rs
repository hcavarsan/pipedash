use std::sync::{
    Arc,
    Mutex,
};

use pipedash_plugin_api::PluginRegistry;
use tokio_util::sync::CancellationToken;

use crate::{
    application,
    domain::DomainResult,
    event::EventBus,
    infrastructure::{
        self,
        database::{
            init_database,
            DatabasePool,
            MetricsRepository,
            Repository,
        },
        TokenStore,
    },
    AlertEvents,
    CoreContext,
};

impl CoreContext {
    pub(super) fn from_services(
        event_bus: Arc<dyn EventBus>, token_store: Arc<dyn TokenStore>,
        http_client_manager: Arc<infrastructure::HttpClientManager>,
        metrics_service: Option<Arc<application::MetricsService>>,
        config_state: Arc<infrastructure::ConfigState>, services: application::Services,
        alert_events: AlertEvents,
    ) -> Self {
        Self {
            alert_events,
            alert_stop: CancellationToken::new(),
            recent_notifications: Arc::default(),
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
            log_service: services.log_service,
            pipeline_view_service: services.pipeline_view_service,
            config_state,
        }
    }

    async fn initialize_services(
        config_state: Arc<infrastructure::ConfigState>,
        config_backend: Arc<dyn infrastructure::ConfigBackend>, cache_pool: DatabasePool,
        token_store: Arc<dyn TokenStore>, event_bus: Arc<dyn EventBus>,
        plugin_registry: Arc<PluginRegistry>,
    ) -> anyhow::Result<Self> {
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

        let config = config_state.get().await;
        let repository = Arc::new(Repository::new(
            config_backend,
            cache_pool.clone(),
            token_store.clone(),
        ));
        let http_client_manager = http_client_manager(&config.general.proxy_url)?;
        let (event_bus, alert_events) = application::recording_bus(event_bus);
        let metrics_service = config.general.metrics_enabled.then(|| {
            let metrics_repository = Arc::new(MetricsRepository::new_from_pool(cache_pool));
            Arc::new(application::MetricsService::new(
                metrics_repository,
                Arc::clone(&event_bus),
            ))
        });

        let log_store = infrastructure::run_logs::LogStore::open(
            config.cache_dir().join("run-logs"),
            infrastructure::run_logs::CACHE_BUDGET_BYTES,
        )
        .await?;
        let services = application::Services::new(
            repository,
            Arc::clone(&http_client_manager),
            metrics_service.clone(),
            Arc::clone(&event_bus),
            Arc::clone(&config_state),
            log_store,
            plugin_registry,
        );

        Ok(Self::from_services(
            event_bus,
            token_store,
            http_client_manager,
            metrics_service,
            config_state,
            services,
            Arc::new(Mutex::new(Some(alert_events))),
        ))
    }

    pub async fn new(
        data_dir: &std::path::Path, event_bus: Arc<dyn EventBus>, token_store: Arc<dyn TokenStore>,
        plugin_registry: Arc<PluginRegistry>,
    ) -> anyhow::Result<Self> {
        tokio::fs::create_dir_all(data_dir).await?;

        let config_state = infrastructure::ConfigState::initialize(
            data_dir,
            token_store.clone(),
            Arc::clone(&plugin_registry),
        )
        .await?;
        let db_path = config_state.get().await.db_path();
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let pool = init_database(db_path).await?;
        let config_backend = Arc::new(infrastructure::SqliteConfigBackend::new(pool.clone()))
            as Arc<dyn infrastructure::ConfigBackend>;

        Self::initialize_services(
            config_state,
            config_backend,
            DatabasePool::Sqlite(pool),
            token_store,
            event_bus,
            plugin_registry,
        )
        .await
    }

    pub async fn with_storage_manager(
        storage_manager: &infrastructure::StorageManager, event_bus: Arc<dyn EventBus>,
        plugin_registry: Arc<PluginRegistry>,
    ) -> anyhow::Result<Self> {
        let token_store = storage_manager.token_store().await;
        let data_dir = storage_manager.config().data_dir();
        let config_state = infrastructure::ConfigState::initialize(
            &data_dir,
            token_store.clone(),
            Arc::clone(&plugin_registry),
        )
        .await?;

        Self::initialize_services(
            config_state,
            storage_manager.config_backend(),
            storage_manager.database_pool(),
            token_store,
            event_bus,
            plugin_registry,
        )
        .await
    }
}

pub(super) fn http_client_manager(
    proxy_url: &str,
) -> DomainResult<Arc<infrastructure::HttpClientManager>> {
    let manager = infrastructure::HttpClientManager::new()?;
    if let Err(e) = manager.set_proxy(proxy_url) {
        tracing::error!(error = %e, "Ignoring general.proxy_url from config.toml");
    }

    Ok(Arc::new(manager))
}
