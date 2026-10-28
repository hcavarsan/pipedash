pub mod application;
pub mod domain;
pub mod event;
pub mod infrastructure;
pub mod logging;
pub mod plugins;
#[cfg(test)]
mod test_env;

use std::sync::Arc;

pub use application::{
    LogExport,
    LogService,
};
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

    pub log_service: Arc<application::LogService>,

    pub pipeline_view_service: Arc<application::PipelineViewService>,

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
            log_service: services.log_service,
            pipeline_view_service: services.pipeline_view_service,
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

    pub async fn factory_reset(&self) -> DomainResult<usize> {
        let providers = self.provider_service.list_providers().await?;
        let providers_removed = providers.len();
        for provider in providers {
            self.provider_service.remove_provider(provider.id).await?;
        }

        for key in self.config_state.load_providers()?.keys() {
            self.config_state.remove_provider(key).await?;
        }
        for provider_id in self.token_store.get_all_tokens().await?.into_keys() {
            self.token_store.delete_token(provider_id).await?;
        }
        self.pipeline_service.clear_all_caches_atomic().await?;
        self.pipeline_view_service.reset().await?;
        self.log_service.clear().await?;
        let metrics = MetricsRepository::new_from_pool(
            self.provider_service.repository().cache_pool().clone(),
        );
        metrics.flush_metrics(None, true).await?;
        metrics.reset_all_processing_states().await?;
        Ok(providers_removed)
    }

    pub async fn shutdown(&self) {
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

fn http_client_manager(proxy_url: &str) -> DomainResult<Arc<infrastructure::HttpClientManager>> {
    let manager = infrastructure::HttpClientManager::new()?;
    if let Err(e) = manager.set_proxy(proxy_url) {
        tracing::error!(error = %e, "Ignoring general.proxy_url from config.toml");
    }

    Ok(Arc::new(manager))
}

#[cfg(test)]
mod reset_tests {
    use std::collections::HashMap;

    use super::*;
    use crate::application::test_support::FailOnceTokenStore;
    use crate::infrastructure::{
        ConfigLoader,
        ConfigState,
        PipedashConfig,
        ProviderFileConfig,
        SqliteConfigBackend,
    };

    async fn context(dir: &std::path::Path, tokens: Arc<dyn TokenStore>) -> CoreContext {
        let mut config = PipedashConfig::default();
        config.storage.data_dir = dir.display().to_string();
        config.general.metrics_enabled = false;
        config.general.default_refresh_interval = 73;
        let path = dir.join("config.toml");
        std::fs::write(&path, ConfigLoader::to_toml(&config).unwrap()).unwrap();
        let config_state = ConfigState::from_config(config, path, Arc::clone(&tokens));
        let pool = init_database(dir.join("pipedash.db")).await.unwrap();
        let repository = Arc::new(Repository::new(
            Arc::new(SqliteConfigBackend::new(pool.clone())),
            infrastructure::database::DatabasePool::Sqlite(pool),
            Arc::clone(&tokens),
        ));
        let bus: Arc<dyn EventBus> = Arc::new(NoOpEventBus);
        let http = http_client_manager("").unwrap();
        let log_store = infrastructure::run_logs::LogStore::open(dir.join("run-logs"), u64::MAX)
            .await
            .unwrap();
        let services = application::Services::new(
            repository,
            Arc::clone(&http),
            None,
            Arc::clone(&bus),
            Arc::clone(&config_state),
            log_store,
        );
        CoreContext::from_services(bus, tokens, http, None, config_state, services)
    }

    async fn define_provider(core: &CoreContext, key: &str) {
        core.config_state
            .add_provider(
                key.into(),
                ProviderFileConfig {
                    name: None,
                    provider_type: "jenkins".into(),
                    token: "storage:1".into(),
                    refresh_interval: 30,
                    config: HashMap::new(),
                },
            )
            .await
            .unwrap();
    }

    fn provider(key: &str) -> ProviderConfig {
        ProviderConfig {
            id: None,
            name: key.into(),
            provider_type: "jenkins".into(),
            token: "saved-token".into(),
            config: HashMap::new(),
            refresh_interval: 30,
            version: None,
        }
    }

    #[tokio::test]
    async fn reset_removes_config_only_providers_and_persisted_metrics_when_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let tokens = Arc::new(FailOnceTokenStore::default());
        let core = context(dir.path(), tokens.clone()).await;
        define_provider(&core, "ci-a").await;
        define_provider(&core, "config-only").await;
        core.provider_service
            .repository()
            .add_provider(&provider("ci-a"))
            .await
            .unwrap();
        tokens.store_token(999, "orphan-token").await.unwrap();
        let pool = core
            .provider_service
            .repository()
            .cache_pool()
            .as_sqlite()
            .unwrap();
        sqlx::query("INSERT INTO pipeline_metrics (pipeline_id, timestamp, metric_type, value) VALUES ('retained', '2026-10-26T00:00:00Z', 'duration', 1)").execute(pool).await.unwrap();
        sqlx::query("INSERT INTO metrics_processing_state (pipeline_id) VALUES ('retained')")
            .execute(pool)
            .await
            .unwrap();
        std::fs::write(dir.path().join("export.toml"), "user export").unwrap();

        assert_eq!(core.factory_reset().await.unwrap(), 1);

        assert!(core
            .provider_service
            .list_providers()
            .await
            .unwrap()
            .is_empty());
        assert!(tokens.get_all_tokens().await.unwrap().is_empty());
        core.config_state.reload().await.unwrap();
        assert!(core.config_state.get().await.providers.is_empty());
        assert_eq!(
            core.config_state
                .get()
                .await
                .general
                .default_refresh_interval,
            73
        );
        core.config_state
            .sync_providers_from_toml(
                core.provider_service.repository().config_backend().as_ref(),
                false,
            )
            .await
            .unwrap();
        assert!(core
            .provider_service
            .list_providers()
            .await
            .unwrap()
            .is_empty());
        let metrics: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pipeline_metrics")
            .fetch_one(pool)
            .await
            .unwrap();
        let processing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM metrics_processing_state")
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!((metrics, processing), (0, 0));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("export.toml")).unwrap(),
            "user export"
        );
        assert_eq!(core.factory_reset().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn reset_stops_on_token_deletion_failure_and_can_be_retried() {
        let dir = tempfile::tempdir().unwrap();
        let tokens = Arc::new(FailOnceTokenStore::default());
        let core = context(dir.path(), tokens.clone()).await;
        define_provider(&core, "ci-a").await;
        let id = core
            .provider_service
            .repository()
            .add_provider(&provider("ci-a"))
            .await
            .unwrap();
        tokens.fail_next_delete();

        assert!(core.factory_reset().await.is_err());
        assert!(core
            .provider_service
            .repository()
            .get_provider(id)
            .await
            .is_ok());
        assert!(core
            .config_state
            .load_providers()
            .unwrap()
            .contains_key("ci-a"));

        assert_eq!(core.factory_reset().await.unwrap(), 1);
        assert!(tokens.get_all_tokens().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn reset_reports_cache_and_metrics_failures_and_supports_retry() {
        for table in ["workflow_parameters_cache", "pipeline_metrics"] {
            let dir = tempfile::tempdir().unwrap();
            let core = context(dir.path(), Arc::new(MemoryTokenStore::new())).await;
            let pool = core
                .provider_service
                .repository()
                .cache_pool()
                .as_sqlite()
                .unwrap();
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "ALTER TABLE {table} RENAME TO retained_data"
            )))
            .execute(pool)
            .await
            .unwrap();

            assert!(core.factory_reset().await.is_err(), "{table}");

            sqlx::query(sqlx::AssertSqlSafe(format!(
                "ALTER TABLE retained_data RENAME TO {table}"
            )))
            .execute(pool)
            .await
            .unwrap();
            assert_eq!(core.factory_reset().await.unwrap(), 0);
        }
    }
}
