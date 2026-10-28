use std::collections::{
    HashMap,
    HashSet,
};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use dashmap::DashMap;
use pipedash_plugin_api::{
    Plugin as PluginTrait,
    PluginRegistry,
    RateLimit,
    CONTROLLER_HEALTH_FEATURE,
};
use tokio::sync::{
    Mutex,
    RwLock,
};
use tokio::time::{
    timeout,
    Instant,
};

use super::health_service::{
    HealthHandle,
    HealthInput,
    ProviderHealthInput,
};
use crate::application::fetch_scheduler::{
    FetchScheduler,
    ForcedClaim,
    Generation,
};
use crate::application::refresh_manager::RefreshDispatcher;
use crate::domain::{
    DomainError,
    DomainResult,
    FetchStatus,
    HealthLevel,
    Provider,
    ProviderConfig,
    ProviderFreshness,
    ProviderHealth,
    ProviderImportResult,
    ProviderSummary,
};
use crate::event::{
    CoreEvent,
    EventBus,
};
use crate::infrastructure::config::sync::{
    ProviderSyncService,
    SyncResult,
};
use crate::infrastructure::config::token_ref::TokenReference;
use crate::infrastructure::config::{
    ConfigLoader,
    ConfigState,
    GeneralSettingsUpdate,
    PipedashConfig,
    ProviderFileConfig,
};
use crate::infrastructure::database::{
    Repository,
    FETCH_STATUS_ERROR,
    FETCH_STATUS_NEVER,
    FETCH_STATUS_SUCCESS,
};
use crate::infrastructure::providers::PluginAdapter;
use crate::infrastructure::run_logs::LogStore;
use crate::infrastructure::{
    HttpClientManager,
    TokenStore,
};
use crate::plugins;

const QUOTA_PAUSE_PERCENT: u64 = 10;
const ACQUIRE_ATTEMPTS: usize = 3;

pub(crate) type ProviderFactory =
    Arc<dyn Fn(&ProviderConfig) -> DomainResult<Arc<dyn Provider>> + Send + Sync>;

#[derive(Clone)]
pub struct ProviderLease {
    pub id: i64,
    pub generation: Generation,
    pub provider: Arc<dyn Provider>,
}

struct ProviderSlot {
    generation: Generation,
    provider: Arc<dyn Provider>,
}

impl ProviderSlot {
    fn lease(&self, id: i64) -> ProviderLease {
        ProviderLease {
            id,
            generation: self.generation,
            provider: Arc::clone(&self.provider),
        }
    }
}

struct ParameterFetch<'a> {
    fetches: &'a DashMap<String, Arc<Mutex<()>>>,
    key: &'a str,
    lock: Arc<Mutex<()>>,
}

impl<'a> ParameterFetch<'a> {
    fn new(fetches: &'a DashMap<String, Arc<Mutex<()>>>, key: &'a str) -> Self {
        let lock = Arc::clone(fetches.entry(key.to_string()).or_default().value());

        Self { fetches, key, lock }
    }
}

impl Drop for ParameterFetch<'_> {
    fn drop(&mut self) {
        self.fetches
            .remove_if(self.key, |_, lock| Arc::ptr_eq(lock, &self.lock));
    }
}

pub struct ProviderService {
    repository: Arc<Repository>,
    config_state: Arc<ConfigState>,
    http_client_manager: Arc<HttpClientManager>,
    providers: RwLock<HashMap<i64, ProviderSlot>>,
    commit_locks: DashMap<i64, Arc<Mutex<()>>>,
    plugin_registry: Arc<PluginRegistry>,
    parameter_fetches: DashMap<String, Arc<Mutex<()>>>,
    event_bus: Arc<dyn EventBus>,
    scheduler: Arc<FetchScheduler>,
    health: HealthHandle,
    dispatcher: RefreshDispatcher,
    provider_factory: ProviderFactory,
    config_apply: Mutex<()>,
    log_store: Arc<LogStore>,
}

impl ProviderService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        repository: Arc<Repository>, http_client_manager: Arc<HttpClientManager>,
        event_bus: Arc<dyn EventBus>, scheduler: Arc<FetchScheduler>, health: HealthHandle,
        dispatcher: RefreshDispatcher, config_state: Arc<ConfigState>, log_store: Arc<LogStore>,
    ) -> Self {
        let plugin_registry = Arc::new(plugins::create_plugin_registry());
        let provider_factory: ProviderFactory = {
            let registry = Arc::clone(&plugin_registry);
            let http = Arc::clone(&http_client_manager);
            Arc::new(move |config| plugin_provider(&registry, &http, config))
        };

        Self {
            repository,
            config_state,
            http_client_manager,
            providers: RwLock::new(HashMap::new()),
            commit_locks: DashMap::new(),
            plugin_registry,
            parameter_fetches: DashMap::new(),
            event_bus,
            scheduler,
            health,
            dispatcher,
            provider_factory,
            config_apply: Mutex::new(()),
            log_store,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_provider_factory(mut self, factory: ProviderFactory) -> Self {
        self.provider_factory = factory;
        self
    }

    pub fn repository(&self) -> &Arc<Repository> {
        &self.repository
    }

    pub(crate) fn scheduler(&self) -> &Arc<FetchScheduler> {
        &self.scheduler
    }

    pub(crate) fn health(&self) -> &HealthHandle {
        &self.health
    }

    pub(crate) fn dispatcher(&self) -> &RefreshDispatcher {
        &self.dispatcher
    }

    pub fn list_available_plugins(&self) -> Vec<pipedash_plugin_api::PluginMetadata> {
        let mut metadata_list = Vec::new();
        for provider_type in self.plugin_registry.provider_types() {
            if let Some(plugin) = self.plugin_registry.get(&provider_type) {
                metadata_list.push(plugin.metadata().clone());
            }
        }
        metadata_list
    }

    pub fn retains_run_history(&self, provider_type: &str) -> bool {
        self.plugin_registry
            .get(provider_type)
            .is_some_and(|plugin| plugin.metadata().capabilities.retains_run_history)
    }

    pub fn init_plugin(
        &self, provider_type: &str, config: HashMap<String, String>, provider_id: i64,
    ) -> DomainResult<Box<dyn PluginTrait>> {
        init_plugin(
            &self.http_client_manager,
            provider_type,
            config,
            provider_id,
        )
    }

    pub async fn field_options(
        &self, provider_type: &str, field_key: &str, config: HashMap<String, String>,
    ) -> DomainResult<Vec<String>> {
        let (mut plugin, client) = build_plugin(&self.http_client_manager, provider_type, &config)?;
        let init_error = plugin.initialize(0, config.clone(), Some(client)).err();

        match plugin.get_field_options(field_key, &config).await {
            Ok(options) => Ok(options),
            Err(e) => Err(match init_error {
                Some(init) => {
                    DomainError::InvalidConfig(format!("Failed to initialize plugin: {init}"))
                }
                None => PluginAdapter::map_error(e),
            }),
        }
    }

    pub(crate) fn commit_lock(&self, id: i64) -> Arc<Mutex<()>> {
        Arc::clone(self.commit_locks.entry(id).or_default().value())
    }

    async fn purge_run_logs(&self, id: i64) {
        if let Err(e) = self.log_store.purge_provider(id).await {
            tracing::warn!(provider_id = id, error = %e, "Failed to purge cached run logs");
        }
    }

    async fn current(&self, id: i64) -> Option<ProviderLease> {
        self.providers
            .read()
            .await
            .get(&id)
            .map(|slot| slot.lease(id))
    }

    pub(crate) async fn generation_of(&self, id: i64) -> Option<Generation> {
        self.providers
            .read()
            .await
            .get(&id)
            .map(|slot| slot.generation)
    }

    pub(crate) async fn health_inputs(&self) -> Vec<ProviderHealthInput> {
        let leases: Vec<ProviderLease> = self
            .providers
            .read()
            .await
            .iter()
            .map(|(id, slot)| slot.lease(*id))
            .collect();
        let mut inputs = Vec::with_capacity(leases.len());
        for lease in leases {
            match self.repository.get_provider(lease.id).await {
                Ok(config) => inputs.push(self.health_input(&config, &lease)),
                Err(e) => {
                    tracing::debug!(provider_id = lease.id, error = %e, "Provider has no config row");
                }
            }
        }

        inputs
    }

    async fn install(&self, id: i64, provider: Arc<dyn Provider>, interval: i64) -> ProviderLease {
        let mut providers = self.providers.write().await;
        providers
            .entry(id)
            .or_insert_with(|| {
                let generation = Generation::next();
                self.scheduler
                    .register(id, generation, interval_of(interval));
                ProviderSlot {
                    generation,
                    provider,
                }
            })
            .lease(id)
    }

    async fn replace(&self, id: i64, provider: Arc<dyn Provider>, interval: i64) -> ProviderLease {
        let generation = Generation::next();
        let mut providers = self.providers.write().await;
        providers.insert(
            id,
            ProviderSlot {
                generation,
                provider: Arc::clone(&provider),
            },
        );
        self.scheduler.reset(id, generation, interval_of(interval));

        ProviderLease {
            id,
            generation,
            provider,
        }
    }

    pub async fn acquire(&self, id: i64) -> DomainResult<ProviderLease> {
        if let Some(lease) = self.current(id).await {
            return Ok(lease);
        }

        for _ in 0..ACQUIRE_ATTEMPTS {
            let config = self.repository.get_provider(id).await?;
            let provider = (self.provider_factory)(&config)?;
            let lock = self.commit_lock(id);
            let _guard = lock.lock().await;
            if let Some(lease) = self.current(id).await {
                return Ok(lease);
            }
            let row = self.repository.get_provider(id).await?;
            if row.version == config.version {
                let lease = self.install(id, provider, row.refresh_interval).await;
                self.health.send(HealthInput::ProvidersChanged {
                    changed: vec![self.health_input(&row, &lease)],
                });
                return Ok(lease);
            }
        }

        Err(DomainError::ConcurrentModification(format!(
            "Provider {id} kept changing while it was loaded"
        )))
    }

    pub async fn commit<F, Fut, T>(&self, lease: &ProviderLease, f: F) -> Option<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        self.commit_at(lease.id, lease.generation, f).await
    }

    async fn commit_at<F, Fut, T>(&self, id: i64, generation: Generation, f: F) -> Option<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        let lock = self.commit_lock(id);
        let _guard = lock.lock().await;
        let current = self
            .providers
            .read()
            .await
            .get(&id)
            .map(|slot| slot.generation);
        if current != Some(generation) {
            return None;
        }

        Some(f().await)
    }

    pub async fn publish_summary(&self, id: i64, generation: Generation) {
        self.commit_at(id, generation, || self.emit_status(id))
            .await;
    }

    pub(crate) async fn emit_status(&self, id: i64) {
        match self.summary_of(id).await {
            Ok(provider) => {
                self.event_bus
                    .emit(CoreEvent::ProviderStatusUpdated { provider })
                    .await;
            }
            Err(e) => {
                tracing::warn!(provider_id = id, error = %e, "Failed to build provider summary");
            }
        }
    }

    async fn summary_of(&self, id: i64) -> DomainResult<ProviderSummary> {
        let config = self.repository.get_provider(id).await?;

        self.build_summary(id, &config).await
    }

    async fn build_summary(
        &self, id: i64, config: &ProviderConfig,
    ) -> DomainResult<ProviderSummary> {
        let cached_pipelines = self.repository.get_cached_pipelines(Some(id)).await?;
        let (status, last_fetch_error, last_fetch_at) = match self
            .repository
            .get_provider_fetch_status(id)
            .await
        {
            Ok(status) => status,
            Err(e) => {
                tracing::warn!(provider_id = id, error = %e, "Failed to read provider fetch status");
                (FETCH_STATUS_NEVER.to_string(), None, None)
            }
        };
        let last_fetch_status = match status.as_str() {
            FETCH_STATUS_SUCCESS => FetchStatus::Success,
            FETCH_STATUS_ERROR => FetchStatus::Error,
            _ => FetchStatus::Never,
        };
        let now = Instant::now();
        let wall = Utc::now();
        let view = self.scheduler.view(id, now);
        let generation = self
            .providers
            .read()
            .await
            .get(&id)
            .map(|slot| slot.generation);
        let snapshot = self.health.snapshot();
        let state = snapshot
            .providers
            .get(&id)
            .filter(|state| Some(state.generation) == generation);
        let last_fetch_at = last_fetch_at.and_then(|at| {
            DateTime::parse_from_rfc3339(&at)
                .ok()
                .map(|at| at.with_timezone(&Utc))
        });
        let paused_until = self.scheduler.paused_until(id, wall);
        let level = state.map_or(HealthLevel::Operational, |state| state.level);
        let freshness = ProviderFreshness::compute(
            &last_fetch_status,
            level,
            last_fetch_at,
            paused_until,
            config.refresh_interval,
            wall,
        );

        Ok(ProviderSummary {
            id,
            name: config.display_name().to_string(),
            provider_type: config.provider_type.clone(),
            icon: self
                .plugin_registry
                .get(&config.provider_type)
                .and_then(|plugin| plugin.metadata().icon.clone()),
            pipeline_count: cached_pipelines.len(),
            last_updated: cached_pipelines.iter().map(|p| p.last_updated).max(),
            refresh_interval: config.refresh_interval,
            configured_repositories: configured_repositories(config),
            last_fetch_status,
            last_fetch_error,
            last_fetch_at,
            paused_until,
            last_fetch_error_kind: view.as_ref().and_then(|view| view.last_failure),
            health: ProviderHealth {
                level,
                signals: state.map(|state| state.signals.clone()).unwrap_or_default(),
                next_attempt_at: view
                    .as_ref()
                    .filter(|view| view.failures > 0)
                    .and_then(|_| {
                        let floor = state
                            .filter(|state| state.outage_floor)
                            .map(|_| self.scheduler.policy().outage_floor);
                        self.scheduler.next_due_in(id, now, floor)
                    })
                    .and_then(|wait| chrono::Duration::from_std(wait).ok())
                    .map(|wait| wall + wait),
                in_flight: view.as_ref().is_some_and(|view| view.in_flight),
                status_page: state.and_then(|state| state.status_page.clone()),
            },
            version: config.version.unwrap_or(1),
            freshness,
        })
    }

    fn health_input(&self, config: &ProviderConfig, lease: &ProviderLease) -> ProviderHealthInput {
        ProviderHealthInput {
            id: lease.id,
            generation: Some(lease.generation),
            display_name: config.display_name().to_string(),
            source: lease.provider.status_source().cloned(),
            has_instance_checks: controller_health(&self.plugin_registry, &config.provider_type),
        }
    }

    fn queue_fetch(&self, lease: &ProviderLease) {
        if let ForcedClaim::Claimed(claim) =
            self.scheduler
                .claim_forced(lease.id, lease.generation, Instant::now())
        {
            self.dispatcher.fetch_now(vec![claim]);
        }
    }

    pub async fn add_provider(&self, mut config: ProviderConfig) -> DomainResult<i64> {
        let mut plugin_config = config.config.clone();
        plugin_config.insert("token".to_string(), config.token.clone());

        let plugin = self.init_plugin(&config.provider_type, plugin_config, 0)?;

        let (validation_result, permission_status) =
            tokio::join!(plugin.validate_credentials(), plugin.check_permissions());

        validation_result.map_err(|e| {
            DomainError::InvalidConfig(format!("Credential validation failed: {e}"))
        })?;

        let provider_type = plugin.provider_type();
        if provider_type != config.provider_type {
            return Err(DomainError::InvalidConfig(format!(
                "Provider type mismatch: expected '{}', got '{}'",
                config.provider_type, provider_type
            )));
        }

        keep_selection_with_ci(plugin.as_ref(), &mut config).await?;

        let permission_status = permission_status.ok();

        let id = self.repository.add_provider(&config).await?;

        if let Some(status) = &permission_status {
            if let Err(e) = self.repository.store_provider_permissions(id, status).await {
                tracing::warn!(provider_id = id, error = %e, "Failed to store permissions");
            }
        }

        let mut config_with_id = config.clone();
        config_with_id.id = Some(id);
        let provider = (self.provider_factory)(&config_with_id)?;

        let lock = self.commit_lock(id);
        let _guard = lock.lock().await;
        let lease = self.install(id, provider, config.refresh_interval).await;
        self.health.send(HealthInput::ProvidersChanged {
            changed: vec![self.health_input(&config_with_id, &lease)],
        });
        let provider = self.summary_of(id).await?;
        self.event_bus
            .emit(CoreEvent::ProviderAdded {
                provider,
                timestamp: Utc::now().timestamp_millis(),
            })
            .await;

        Ok(id)
    }

    pub async fn export_providers_toml(&self) -> DomainResult<String> {
        let providers = self.repository.config_backend().list_providers().await?;

        ProviderSyncService::export_toml(&providers)
    }

    pub async fn import_providers_toml(&self, content: &str) -> DomainResult<ProviderImportResult> {
        let file = ConfigLoader::parse_raw(content).map_err(|e| {
            DomainError::InvalidConfig(format!("Couldn't read the providers file: {e}"))
        })?;
        if file.providers.is_empty() {
            return Err(DomainError::InvalidConfig(
                "The file has no [providers] entries".to_string(),
            ));
        }
        if let Some((key, provider)) = file
            .providers
            .iter()
            .find(|(_, provider)| !self.plugin_registry.is_registered(&provider.provider_type))
        {
            return Err(DomainError::InvalidProviderType(format!(
                "Provider '{key}' has an unknown type '{}'",
                provider.provider_type
            )));
        }

        let existing: HashSet<String> = self
            .repository
            .config_backend()
            .list_providers()
            .await?
            .into_iter()
            .map(|provider| provider.name)
            .collect();
        let mut result = ProviderImportResult::default();
        let mut failure = None;

        for (key, provider) in &file.providers {
            let name = provider.display_name(key).to_string();
            if existing.contains(key) {
                result.skipped.push(name);
                continue;
            }

            let config = ProviderSyncService::provider_config(key, provider, String::new());
            match self.repository.add_provider(&config).await {
                Ok(_) => result.added.push(name),
                Err(e) => {
                    let added_elsewhere = matches!(e, DomainError::InvalidConfig(_))
                        && self
                            .repository
                            .config_backend()
                            .list_providers()
                            .await
                            .is_ok_and(|providers| providers.iter().any(|p| p.name == *key));
                    if !added_elsewhere {
                        failure = Some(e);
                        break;
                    }
                    result.skipped.push(name);
                }
            }
        }

        if !result.added.is_empty() {
            self.event_bus.emit(CoreEvent::ProvidersChanged).await;
        }

        failure.map_or(Ok(result), Err)
    }

    pub async fn apply_config_file(&self, config_state: &ConfigState) -> DomainResult<SyncResult> {
        let _apply = self.config_apply.lock().await;
        let PipedashConfig {
            providers: file,
            views,
            ..
        } = config_state.load_file()?;
        let current = config_state.get().await;
        let changed: Vec<(String, ProviderFileConfig)> = file
            .iter()
            .filter(|(key, provider)| current.providers.get(*key) != Some(*provider))
            .map(|(key, provider)| (key.clone(), provider.clone()))
            .collect();

        for (key, provider) in &changed {
            if !self.plugin_registry.is_registered(&provider.provider_type) {
                return Err(DomainError::InvalidProviderType(format!(
                    "Provider '{key}' has an unknown type '{}'",
                    provider.provider_type
                )));
            }
            (self.provider_factory)(&ProviderSyncService::provider_config(
                key,
                provider,
                provider.token.clone(),
            ))?;
        }

        let mut result = SyncResult::default();
        if !changed.is_empty() {
            let existing = self.repository.config_backend().list_providers().await?;
            let token_store = config_state.token_store();
            for (key, provider) in &changed {
                let id = existing.iter().find(|p| p.name == *key).and_then(|p| p.id);
                self.apply_config_provider(key, provider, id, token_store.as_ref())
                    .await?;
                config_state.remember_provider(key, provider).await;
                match id {
                    Some(_) => result.updated.push(key.clone()),
                    None => result.added.push(key.clone()),
                }
            }

            tracing::info!(
                added = result.added.len(),
                updated = result.updated.len(),
                "Applied provider changes from config file"
            );
            self.event_bus.emit(CoreEvent::ProvidersChanged).await;
        }
        if current.views != views {
            config_state.remember_views(views).await;
            self.event_bus.emit(CoreEvent::PipelineViewsChanged).await;
        }
        config_state.remember_providers(file).await;

        Ok(result)
    }

    pub async fn update_general_settings(
        &self, config_state: &ConfigState, update: GeneralSettingsUpdate,
    ) -> DomainResult<()> {
        let proxy_url = update.proxy_url.clone();
        config_state.update_general_settings(update).await?;
        if self.http_client_manager.set_proxy(&proxy_url)? {
            self.rebuild_loaded_providers().await;
        }

        Ok(())
    }

    async fn rebuild_loaded_providers(&self) {
        let ids: Vec<i64> = self.providers.read().await.keys().copied().collect();
        let mut changed = Vec::new();

        for id in ids {
            let lock = self.commit_lock(id);
            let _guard = lock.lock().await;
            if !self.providers.read().await.contains_key(&id) {
                continue;
            }

            let rebuilt = match self.repository.get_provider(id).await {
                Ok(config) => (self.provider_factory)(&config).map(|provider| (config, provider)),
                Err(e) => Err(e),
            };
            match rebuilt {
                Ok((config, provider)) => {
                    let lease = self.replace(id, provider, config.refresh_interval).await;
                    changed.push(self.health_input(&config, &lease));
                }
                Err(e) => {
                    tracing::warn!(provider_id = id, error = %e, "Failed to rebuild provider");
                }
            }
        }

        if !changed.is_empty() {
            self.health.send(HealthInput::ProvidersChanged { changed });
        }
    }

    async fn apply_config_provider(
        &self, key: &str, provider: &ProviderFileConfig, existing: Option<i64>,
        token_store: &dyn TokenStore,
    ) -> DomainResult<()> {
        let sync = ProviderSyncService::new();
        let backend = self.repository.config_backend().as_ref();
        let id = match existing {
            Some(id) => id,
            None => sync.create_from_toml(key, provider, backend).await?,
        };

        let lock = self.commit_lock(id);
        let guard = lock.lock().await;
        if existing.is_some() {
            let row = backend
                .get_provider(id)
                .await?
                .ok_or_else(|| DomainError::ProviderNotFound(id.to_string()))?;
            sync.update_from_toml(key, provider, &row, backend).await?;
        }
        ProviderSyncService::store_resolved_token(
            id,
            &provider.token,
            existing.is_none(),
            token_store,
        )
        .await?;

        let config = self.repository.get_provider(id).await?;
        let instance = (self.provider_factory)(&config)?;
        if let Err(e) = self.repository.reset_provider_fetch_status(id).await {
            tracing::warn!(provider_id = id, error = %e, "Failed to reset fetch status");
        }
        let lease = self.replace(id, instance, config.refresh_interval).await;
        self.purge_run_logs(id).await;
        self.health.send(HealthInput::ProvidersChanged {
            changed: vec![self.health_input(&config, &lease)],
        });
        drop(guard);

        self.queue_fetch(&lease);

        Ok(())
    }

    pub async fn get_provider_config(&self, id: i64) -> DomainResult<ProviderConfig> {
        self.repository.get_provider(id).await
    }

    pub async fn list_providers(&self) -> DomainResult<Vec<ProviderSummary>> {
        let configs = self.repository.list_providers().await?;
        let mut summaries = Vec::with_capacity(configs.len());

        for config in configs {
            if let Some(id) = config.id {
                summaries.push(self.build_summary(id, &config).await?);
            }
        }

        Ok(summaries)
    }

    pub async fn update_provider(&self, id: i64, mut config: ProviderConfig) -> DomainResult<()> {
        let saved_name = self.repository.get_provider(id).await?.name;
        if self
            .config_state
            .get()
            .await
            .providers
            .contains_key(&saved_name)
        {
            let display_name = std::mem::replace(&mut config.name, saved_name);
            config
                .config
                .insert("display_name".to_string(), display_name);
        }
        let mut plugin_config = config.config.clone();
        plugin_config.insert("token".to_string(), config.token.clone());

        let plugin = self.init_plugin(&config.provider_type, plugin_config, id)?;

        plugin.validate_credentials().await.map_err(|e| {
            DomainError::InvalidConfig(format!("Credential validation failed: {e}"))
        })?;

        keep_selection_with_ci(plugin.as_ref(), &mut config).await?;

        let permission_status = plugin.check_permissions().await.ok();

        let mut config_with_id = config.clone();
        config_with_id.id = Some(id);
        let new_provider = (self.provider_factory)(&config_with_id)?;

        let lock = self.commit_lock(id);
        let guard = lock.lock().await;

        let current = self.repository.get_provider(id).await?;
        let success = self
            .repository
            .update_provider_with_version(id, &config, current.version.unwrap_or(1))
            .await?;

        if !success {
            return Err(DomainError::ConcurrentModification(format!(
                "Provider {} was modified by another client",
                id
            )));
        }

        if let Err(e) = self.repository.reset_provider_fetch_status(id).await {
            tracing::warn!(provider_id = id, error = %e, "Failed to reset fetch status");
        }
        let lease = self
            .replace(id, new_provider, config.refresh_interval)
            .await;
        self.purge_run_logs(id).await;
        self.health.send(HealthInput::ProvidersChanged {
            changed: vec![self.health_input(&config_with_id, &lease)],
        });
        match self.summary_of(id).await {
            Ok(provider) => {
                self.event_bus
                    .emit(CoreEvent::ProviderUpdated {
                        provider,
                        timestamp: Utc::now().timestamp_millis(),
                    })
                    .await;
            }
            Err(e) => {
                tracing::warn!(provider_id = id, error = %e, "Failed to build updated provider summary");
                self.event_bus.emit(CoreEvent::ProvidersChanged).await;
            }
        }
        drop(guard);

        if let Some(status) = &permission_status {
            if let Err(e) = self.repository.store_provider_permissions(id, status).await {
                tracing::warn!(provider_id = id, error = %e, "Failed to store permissions");
            }
        }

        self.queue_fetch(&lease);

        Ok(())
    }

    pub async fn update_provider_refresh_interval(
        &self, id: i64, refresh_interval: i64,
    ) -> DomainResult<()> {
        let lock = self.commit_lock(id);
        let _guard = lock.lock().await;

        let mut config = self.repository.get_provider(id).await?;
        config.refresh_interval = refresh_interval;
        self.repository.update_provider(id, &config).await?;

        let generation = self
            .providers
            .read()
            .await
            .get(&id)
            .map(|slot| slot.generation);
        if let Some(generation) = generation {
            self.scheduler
                .set_interval(id, generation, interval_of(refresh_interval));
        }

        Ok(())
    }

    pub async fn remove_provider(&self, id: i64) -> DomainResult<()> {
        let _apply = self.config_apply.lock().await;
        let lock = self.commit_lock(id);
        let _guard = lock.lock().await;

        let config = self.repository.get_provider(id).await?;
        let provider = self.build_summary(id, &config).await?;

        self.repository
            .remove_provider(id, &self.config_state)
            .await?;
        self.providers.write().await.remove(&id);
        self.purge_run_logs(id).await;
        self.scheduler.remove(id);
        self.health.send(HealthInput::ProvidersChanged {
            changed: vec![ProviderHealthInput {
                id,
                generation: None,
                display_name: config.display_name().to_string(),
                source: None,
                has_instance_checks: false,
            }],
        });
        self.event_bus
            .emit(CoreEvent::ProviderRemoved {
                provider,
                timestamp: Utc::now().timestamp_millis(),
            })
            .await;

        Ok(())
    }

    pub async fn get_provider(&self, id: i64) -> DomainResult<Arc<dyn Provider>> {
        Ok(self.acquire(id).await?.provider)
    }

    pub async fn check_quota(&self, lease: &ProviderLease) -> Option<DateTime<Utc>> {
        let rate_limit =
            match timeout(Duration::from_secs(10), lease.provider.fetch_rate_limit()).await {
                Ok(Ok(rate_limit)) => rate_limit,
                Ok(Err(e)) => {
                    tracing::debug!(provider_id = lease.id, error = %e, "Rate limit check failed");
                    None
                }
                Err(_) => {
                    tracing::debug!(provider_id = lease.id, "Rate limit check timed out");
                    None
                }
            };
        let paused_until = rate_limit.and_then(|rate_limit| quota_pause(&rate_limit, Utc::now()));

        self.commit(lease, || async {
            let changed = self.scheduler.set_quota_pause(
                lease.id,
                lease.generation,
                paused_until,
                Instant::now(),
                Utc::now(),
            );
            if let (true, Some(until)) = (changed, paused_until) {
                tracing::info!(provider_id = lease.id, %until, "API quota almost used up, pausing provider");
            }
            if changed {
                self.emit_status(lease.id).await;
            }

            paused_until
        })
        .await
        .flatten()
    }

    pub async fn get_provider_instance(&self, id: i64) -> DomainResult<Arc<dyn Provider>> {
        self.get_provider(id).await
    }

    pub async fn load_missing_providers(&self) {
        let ids = match self.repository.list_provider_ids().await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to list providers");
                return;
            }
        };
        let loaded: Vec<i64> = self.providers.read().await.keys().copied().collect();
        for id in ids.into_iter().filter(|id| !loaded.contains(id)) {
            if let Err(e) = self.acquire(id).await {
                tracing::debug!(provider_id = id, error = %e, "Provider not loaded yet");
            }
        }
    }

    pub async fn load_all_providers(&self) -> DomainResult<()> {
        let configs = self.repository.list_providers().await?;
        let mut changed = Vec::new();

        for config in configs {
            let Some(id) = config.id else {
                continue;
            };
            if config.token.is_empty() {
                tracing::debug!(
                    provider_id = id,
                    provider_type = %config.provider_type,
                    "Skipping provider load - token not available yet (vault initializing in background)"
                );
                continue;
            }

            match (self.provider_factory)(&config) {
                Ok(provider) => {
                    let lock = self.commit_lock(id);
                    let _guard = lock.lock().await;
                    let lease = self.replace(id, provider, config.refresh_interval).await;
                    changed.push(self.health_input(&config, &lease));
                }
                Err(e) => {
                    tracing::warn!(provider_id = id, error = %e, "Failed to load provider");
                }
            }
        }

        if !changed.is_empty() {
            self.health.send(HealthInput::ProvidersChanged { changed });
        }

        Ok(())
    }

    pub async fn get_workflow_parameters(
        &self, provider_id: i64, workflow_id: &str,
    ) -> DomainResult<Vec<pipedash_plugin_api::WorkflowParameter>> {
        if let Some(cached) = self.cached_parameters(workflow_id).await {
            return Ok(cached);
        }

        let lease = self.acquire(provider_id).await?;
        let fetch = ParameterFetch::new(&self.parameter_fetches, workflow_id);
        let _guard = fetch.lock.lock().await;

        if let Some(cached) = self.cached_parameters(workflow_id).await {
            return Ok(cached);
        }

        let parameters = lease.provider.get_workflow_parameters(workflow_id).await?;

        let cached = self
            .commit(&lease, || {
                self.repository
                    .cache_workflow_parameters(workflow_id, &parameters)
            })
            .await;
        if let Some(Err(e)) = cached {
            tracing::warn!(provider_id, workflow_id, error = %e, "Failed to cache workflow parameters");
        }

        Ok(parameters)
    }

    async fn cached_parameters(
        &self, workflow_id: &str,
    ) -> Option<Vec<pipedash_plugin_api::WorkflowParameter>> {
        match self
            .repository
            .get_cached_workflow_parameters(workflow_id)
            .await
        {
            Ok(cached) => cached,
            Err(e) => {
                tracing::warn!(workflow_id, error = %e, "Failed to read cached workflow parameters");
                None
            }
        }
    }

    pub async fn get_provider_permissions(
        &self, provider_id: i64,
    ) -> DomainResult<Option<pipedash_plugin_api::PermissionStatus>> {
        self.repository.get_provider_permissions(provider_id).await
    }

    pub async fn recheck_provider_permissions(
        &self, provider_id: i64,
    ) -> DomainResult<pipedash_plugin_api::PermissionStatus> {
        let config = self.repository.get_provider(provider_id).await?;
        let mut plugin_config = config.config.clone();
        plugin_config.insert("token".to_string(), config.token.clone());

        let plugin = self.init_plugin(&config.provider_type, plugin_config, provider_id)?;

        let status = plugin
            .check_permissions()
            .await
            .map_err(|e| DomainError::InvalidConfig(format!("Failed to check permissions: {e}")))?;

        self.repository
            .store_provider_permissions(provider_id, &status)
            .await?;

        Ok(status)
    }
}

fn interval_of(seconds: i64) -> Duration {
    Duration::from_secs(u64::try_from(seconds).unwrap_or(0))
}

fn build_plugin(
    http: &HttpClientManager, provider_type: &str, config: &HashMap<String, String>,
) -> DomainResult<(Box<dyn PluginTrait>, Arc<reqwest::Client>)> {
    let plugin = plugins::create_plugin(provider_type).ok_or_else(|| {
        DomainError::InvalidProviderType(format!("Unknown provider type: {}", provider_type))
    })?;
    let client = http.client_for(&plugin.client_options(config))?;

    Ok((plugin, client))
}

async fn keep_selection_with_ci(
    plugin: &dyn PluginTrait, config: &mut ProviderConfig,
) -> DomainResult<()> {
    let Some(selected) = config.config.get("selected_items") else {
        return Ok(());
    };
    let ids: Vec<String> = selected
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    let Ok(kept) = plugin.ids_with_ci(ids.clone()).await else {
        return Ok(());
    };
    if kept.is_empty() {
        return Err(DomainError::InvalidConfig(
            "None of the selected repositories has a CI file or a pipeline run yet".to_string(),
        ));
    }
    if kept.len() < ids.len() {
        tracing::info!(
            selected = ids.len(),
            kept = kept.len(),
            "Skipped selected repositories without a CI file or a pipeline run"
        );
        config
            .config
            .insert("selected_items".to_string(), kept.join(","));
    }
    Ok(())
}

fn init_plugin(
    http: &HttpClientManager, provider_type: &str, config: HashMap<String, String>,
    provider_id: i64,
) -> DomainResult<Box<dyn PluginTrait>> {
    let (mut plugin, client) = build_plugin(http, provider_type, &config)?;
    plugin
        .initialize(provider_id, config, Some(client))
        .map_err(|e| DomainError::InvalidConfig(format!("Failed to initialize plugin: {e}")))?;

    Ok(plugin)
}

pub(crate) fn controller_health(registry: &PluginRegistry, provider_type: &str) -> bool {
    registry.get(provider_type).is_some_and(|plugin| {
        plugin
            .metadata()
            .features
            .iter()
            .any(|feature| feature.id == CONTROLLER_HEALTH_FEATURE)
    })
}

pub(crate) fn plugin_provider(
    registry: &PluginRegistry, http: &HttpClientManager, config: &ProviderConfig,
) -> DomainResult<Arc<dyn Provider>> {
    if !registry.is_registered(&config.provider_type) {
        return Err(DomainError::InvalidProviderType(format!(
            "Plugin not found for provider type: {}",
            config.provider_type
        )));
    }

    let provider_id = config.id.unwrap_or(0);
    let mut plugin_config = config.config.clone();

    let resolved_token = if config.token.is_empty() {
        String::new()
    } else {
        match TokenReference::parse(&config.token) {
            Ok(TokenReference::EnvVar(var_name)) => std::env::var(&var_name).unwrap_or_else(|_| {
                tracing::warn!(
                    provider_id = provider_id,
                    var_name = %var_name,
                    "Environment variable not found, using empty token"
                );
                String::new()
            }),
            Ok(TokenReference::None) => String::new(),
            Ok(_) => config.token.clone(),
            Err(_) => config.token.clone(),
        }
    };
    plugin_config.insert("token".to_string(), resolved_token);

    let plugin = init_plugin(http, &config.provider_type, plugin_config, provider_id)?;

    Ok(Arc::new(PluginAdapter::new(
        plugin,
        config.provider_type.clone(),
        provider_id,
    )))
}

fn configured_repositories(config: &ProviderConfig) -> Vec<String> {
    config
        .config
        .get("selected_items")
        .map(|items| {
            if matches!(config.provider_type.as_str(), "argocd" | "fluxcd") {
                return Vec::new();
            }

            items
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .map(|item| {
                    if config.provider_type == "tekton" && item.contains("__") {
                        item.replace("__", "/")
                    } else if config.provider_type == "jenkins" && !item.contains('/') {
                        format!("(root)/{}", item)
                    } else {
                        item
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn quota_pause(rate_limit: &RateLimit, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let low = rate_limit.remaining * 100 < rate_limit.limit * QUOTA_PAUSE_PERCENT;

    (low && rate_limit.reset_at > now).then_some(rate_limit.reset_at)
}

#[cfg(test)]
mod tests {
    use std::io::{
        Read,
        Write,
    };

    use pipedash_plugin_api::PluginError;

    use super::*;
    use crate::application::test_support::{
        pipeline,
        Gate,
        Harness,
        StubProvider,
    };
    use crate::application::FetchResult;
    use crate::domain::PipelineStatus;
    use crate::infrastructure::config::ConfigLoader;
    use crate::test_env::EnvGuard;

    const GITHUB_SECRET: &str = "ghp_exampleSECRET0123456789";
    const GITLAB_SECRET: &str = "glpat-SECRET-VALUE";
    const JENKINS_DEFAULT_SECRET: &str = "hunter2-default";

    fn provider(
        name: &str, provider_type: &str, token: &str, refresh_interval: i64,
        config: &[(&str, &str)],
    ) -> ProviderConfig {
        ProviderConfig {
            id: None,
            name: name.to_string(),
            provider_type: provider_type.to_string(),
            token: token.to_string(),
            config: config
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            refresh_interval,
            version: None,
        }
    }

    struct CiCheck(Option<&'static [&'static str]>);

    #[async_trait::async_trait]
    impl PluginTrait for CiCheck {
        fn metadata(&self) -> &pipedash_plugin_api::PluginMetadata {
            unimplemented!()
        }

        fn initialize(
            &mut self, _: i64, _: HashMap<String, String>, _: Option<Arc<reqwest::Client>>,
        ) -> pipedash_plugin_api::PluginResult<()> {
            Ok(())
        }

        async fn validate_credentials(&self) -> pipedash_plugin_api::PluginResult<bool> {
            Ok(true)
        }

        async fn fetch_pipelines(
            &self,
        ) -> pipedash_plugin_api::PluginResult<Vec<pipedash_plugin_api::Pipeline>> {
            Ok(Vec::new())
        }

        async fn fetch_run_history(
            &self, _: &str, _: usize,
        ) -> pipedash_plugin_api::PluginResult<Vec<pipedash_plugin_api::PipelineRun>> {
            Ok(Vec::new())
        }

        async fn fetch_run_details(
            &self, _: &str, _: i64,
        ) -> pipedash_plugin_api::PluginResult<pipedash_plugin_api::PipelineRun> {
            Err(PluginError::NotSupported(String::new()))
        }

        async fn trigger_pipeline(
            &self, _: pipedash_plugin_api::TriggerParams,
        ) -> pipedash_plugin_api::PluginResult<String> {
            Err(PluginError::NotSupported(String::new()))
        }

        async fn ids_with_ci(
            &self, _: Vec<String>,
        ) -> pipedash_plugin_api::PluginResult<Vec<String>> {
            self.0
                .map(|kept| kept.iter().map(|id| id.to_string()).collect())
                .ok_or_else(|| PluginError::NetworkError("reset".to_string()))
        }
    }

    #[tokio::test]
    async fn a_saved_selection_keeps_only_repositories_with_ci() {
        let selected = &[("selected_items", "acme/api, acme/empty,acme/web")];

        let mut config = provider("gh", "github", "t", 30, selected);
        keep_selection_with_ci(&CiCheck(Some(&["acme/api", "acme/web"])), &mut config)
            .await
            .unwrap();
        assert_eq!(config.config["selected_items"], "acme/api,acme/web");

        let mut config = provider("gh", "github", "t", 30, selected);
        keep_selection_with_ci(&CiCheck(None), &mut config)
            .await
            .unwrap();
        assert_eq!(
            config.config["selected_items"],
            "acme/api, acme/empty,acme/web"
        );

        let mut config = provider("gh", "github", "t", 30, selected);
        let error = keep_selection_with_ci(&CiCheck(Some(&[])), &mut config)
            .await
            .unwrap_err();
        assert!(matches!(error, DomainError::InvalidConfig(_)), "{error}");
        assert_eq!(
            config.config["selected_items"],
            "acme/api, acme/empty,acme/web"
        );
    }

    #[tokio::test]
    async fn test_export_providers_toml_round_trips_without_tokens() {
        let source = Harness::with_real_plugins().await;
        let repository = source.repository.clone();
        let service = &source.provider_service;

        let originals = [
            provider(
                "github-main",
                "github",
                GITHUB_SECRET,
                45,
                &[("selected_items", "acme/api,acme/web")],
            ),
            provider(
                "My Jenkins",
                "jenkins",
                &format!("${{JENKINS_API_TOKEN:-{JENKINS_DEFAULT_SECRET}}}"),
                120,
                &[
                    ("display_name", "Prod Jenkins"),
                    ("server_url", "http://127.0.0.1:9"),
                    ("username", "ci"),
                    ("selected_items", "(root)/deploy"),
                ],
            ),
            provider("gitlab_main", "gitlab", GITLAB_SECRET, 30, &[]),
        ];
        for original in &originals {
            repository.add_provider(original).await.unwrap();
        }

        let exported = service.export_providers_toml().await.unwrap();

        for secret in [GITHUB_SECRET, GITLAB_SECRET, JENKINS_DEFAULT_SECRET] {
            assert!(!exported.contains(secret), "export leaked {secret}");
        }
        assert!(exported.contains("token = \"${PIPEDASH_TOKEN_GITHUB_MAIN:-}\""));
        assert!(exported.contains("token = \"${JENKINS_API_TOKEN:-}\""));
        assert!(exported.contains("token = \"${PIPEDASH_TOKEN_GITLAB_MAIN:-}\""));

        let _env = EnvGuard::set(&[
            ("PIPEDASH_TOKEN_GITHUB_MAIN", Some("env-github-token")),
            ("JENKINS_API_TOKEN", Some("env-jenkins-token")),
            ("PIPEDASH_TOKEN_GITLAB_MAIN", None),
        ]);
        let parsed = ConfigLoader::parse(&exported).unwrap();

        let mut keys: Vec<_> = parsed.providers.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["My Jenkins", "github-main", "gitlab_main"]);
        assert_eq!(parsed.providers["github-main"].token, "env-github-token");
        assert_eq!(parsed.providers["My Jenkins"].token, "env-jenkins-token");
        assert_eq!(parsed.providers["gitlab_main"].token, "");
        assert_eq!(
            parsed.providers["My Jenkins"].name.as_deref(),
            Some("Prod Jenkins")
        );

        let target_harness = Harness::with_real_plugins().await;
        let target = &target_harness.repository;
        crate::infrastructure::config::sync::ProviderSyncService::new()
            .sync_toml_to_db(
                parsed.providers,
                target.config_backend().as_ref(),
                &crate::infrastructure::MemoryTokenStore::new(),
                false,
            )
            .await
            .unwrap();

        let imported = target.config_backend().list_providers().await.unwrap();

        assert_eq!(imported.len(), originals.len());
        for original in &originals {
            let imported = imported.iter().find(|p| p.name == original.name).unwrap();
            assert_eq!(imported.name, original.name);
            assert_eq!(imported.provider_type, original.provider_type);
            assert_eq!(imported.refresh_interval, original.refresh_interval);
            assert_eq!(imported.config, original.config);
        }
    }

    #[tokio::test]
    async fn test_import_providers_toml_adds_missing_providers_without_tokens() {
        let source = Harness::with_real_plugins().await;
        let jenkins = provider(
            "My Jenkins",
            "jenkins",
            "${JENKINS_API_TOKEN}",
            120,
            &[
                ("display_name", "Prod Jenkins"),
                ("server_url", "http://127.0.0.1:9"),
                ("username", "ci"),
                ("selected_items", "(root)/deploy"),
            ],
        );
        for original in [
            provider(
                "github-main",
                "github",
                GITHUB_SECRET,
                45,
                &[("selected_items", "acme/api")],
            ),
            jenkins.clone(),
        ] {
            source.repository.add_provider(&original).await.unwrap();
        }
        let exported = source
            .provider_service
            .export_providers_toml()
            .await
            .unwrap();

        let target = Harness::with_real_plugins().await;
        let kept = provider(
            "github-main",
            "github",
            "target-token",
            300,
            &[("selected_items", "acme/web")],
        );
        let kept_id = target.repository.add_provider(&kept).await.unwrap();

        let _env = EnvGuard::set(&[("JENKINS_API_TOKEN", Some("env-jenkins-token"))]);
        let result = target
            .provider_service
            .import_providers_toml(&exported)
            .await
            .unwrap();

        assert_eq!(
            result,
            ProviderImportResult {
                added: vec!["Prod Jenkins".to_string()],
                skipped: vec!["github-main".to_string()],
            }
        );
        let unchanged = target.repository.get_provider(kept_id).await.unwrap();
        assert_eq!(unchanged.token, kept.token);
        assert_eq!(unchanged.refresh_interval, kept.refresh_interval);
        assert_eq!(unchanged.config, kept.config);

        let imported = target.repository.list_providers().await.unwrap();
        let imported = imported.iter().find(|p| p.name == jenkins.name).unwrap();
        let row = target
            .repository
            .config_backend()
            .get_provider(imported.id.unwrap())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(imported.token, "");
        assert_eq!(row.token, "");
        assert_eq!(imported.provider_type, jenkins.provider_type);
        assert_eq!(imported.refresh_interval, jenkins.refresh_interval);
        assert_eq!(imported.config, jenkins.config);
        assert_eq!(target.bus.count("providers-changed"), 1);
    }

    #[tokio::test]
    async fn test_import_providers_toml_rejects_unknown_type_before_adding() {
        let h = Harness::with_real_plugins().await;
        let content = r#"
[providers.github-main]
type = "github"

[providers.mystery]
type = "travis"
"#;

        let err = h
            .provider_service
            .import_providers_toml(content)
            .await
            .unwrap_err();

        assert!(matches!(err, DomainError::InvalidProviderType(_)), "{err}");
        assert!(h.repository.list_providers().await.unwrap().is_empty());
        assert_eq!(h.bus.count("providers-changed"), 0);
    }

    #[test]
    fn quota_pause_starts_under_ten_percent_until_reset() {
        let now = Utc::now();
        let reset_at = now + chrono::Duration::minutes(20);
        let pause = |remaining, reset_at| {
            quota_pause(
                &RateLimit {
                    limit: 5000,
                    remaining,
                    reset_at,
                },
                now,
            )
        };

        assert_eq!(pause(500, reset_at), None);
        assert_eq!(pause(499, reset_at), Some(reset_at));
        assert_eq!(pause(0, reset_at), Some(reset_at));
        assert_eq!(pause(0, now), None);
        assert_eq!(
            quota_pause(
                &RateLimit {
                    limit: 0,
                    remaining: 0,
                    reset_at,
                },
                now,
            ),
            None
        );
    }

    type StubGate = Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>;

    async fn stub_github(
        remaining: u64, reset: i64, gate: StubGate,
    ) -> (String, Arc<Mutex<Vec<String>>>) {
        use tokio::io::{
            AsyncReadExt,
            AsyncWriteExt,
        };

        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests: Arc<Mutex<Vec<String>>> = Arc::default();
        let log = requests.clone();
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    let count = stream.read(&mut buffer).await.unwrap();
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..count]);
                }
                let request = String::from_utf8(request).unwrap();
                let path = request.split_whitespace().nth(1).unwrap_or_default();
                let path = path.split('?').next().unwrap().to_string();
                let (status, body) = if path == "/api/v3/rate_limit" {
                    if let Some((arrived, release)) = &gate {
                        arrived.notify_one();
                        release.notified().await;
                    }
                    let rate = serde_json::json!({
                        "limit": 5000, "used": 5000 - remaining, "remaining": remaining, "reset": reset
                    });
                    let search = serde_json::json!({"limit": 30, "used": 0, "remaining": 30, "reset": reset});
                    let body = serde_json::json!({
                        "resources": {"core": rate, "search": search},
                        "rate": rate
                    });
                    ("200 OK", body.to_string())
                } else {
                    ("404 Not Found", r#"{"message":"Not Found"}"#.to_string())
                };
                log.lock().await.push(path);
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });

        (format!("http://{addr}"), requests)
    }

    #[tokio::test]
    async fn refresh_skips_a_github_provider_with_its_quota_almost_used_up() {
        let h = Harness::with_real_plugins().await;
        h.start().await;
        let reset = Utc::now().timestamp() + 1200;
        let (base_url, requests) = stub_github(0, reset, None).await;
        let id = h
            .repository
            .add_provider(&provider(
                "github",
                "github",
                "ghp_stub",
                30,
                &[("base_url", &base_url), ("selected_items", "acme/app")],
            ))
            .await
            .unwrap();

        let first = h
            .pipeline_service
            .fetch_now(&[id], Duration::from_secs(5))
            .await;
        let again = h
            .pipeline_service
            .fetch_now(&[id], Duration::from_secs(5))
            .await;
        let summaries = h.provider_service.list_providers().await.unwrap();

        assert_eq!(*requests.lock().await, ["/api/v3/rate_limit"]);
        assert_eq!(
            summaries[0].paused_until,
            DateTime::from_timestamp(reset, 0)
        );
        assert!(matches!(summaries[0].last_fetch_status, FetchStatus::Never));
        assert_eq!(first[0].state, crate::domain::RefreshState::Recent);
        assert_eq!(again[0].state, crate::domain::RefreshState::Recent);
        assert_eq!(again[0].kind, Some(crate::domain::FailureKind::RateLimited));
        assert_eq!(*requests.lock().await, ["/api/v3/rate_limit"]);
    }

    #[tokio::test]
    async fn refresh_fetches_a_github_provider_with_quota_left() {
        let h = Harness::with_real_plugins().await;
        h.start().await;
        let (base_url, requests) = stub_github(4000, Utc::now().timestamp() + 1200, None).await;
        let id = h
            .repository
            .add_provider(&provider(
                "github",
                "github",
                "ghp_stub",
                30,
                &[("base_url", &base_url), ("selected_items", "acme/app")],
            ))
            .await
            .unwrap();

        h.pipeline_service
            .fetch_now(&[id], Duration::from_secs(5))
            .await;
        let summaries = h.provider_service.list_providers().await.unwrap();
        let requests = requests.lock().await;

        assert_eq!(requests[0], "/api/v3/rate_limit");
        assert!(requests
            .iter()
            .any(|path| path.starts_with("/api/v3/repos/acme/app")));
        assert_eq!(summaries[0].paused_until, None);
    }

    #[tokio::test]
    async fn quota_answer_for_replaced_credentials_does_not_pause_the_new_ones() {
        let h = Harness::with_real_plugins().await;
        let reset = Utc::now().timestamp() + 1200;
        let arrived = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let (base_url, _) = stub_github(0, reset, Some((arrived.clone(), release.clone()))).await;
        let id = h
            .repository
            .add_provider(&provider(
                "github",
                "github",
                "ghp_old",
                30,
                &[("base_url", &base_url), ("selected_items", "acme/app")],
            ))
            .await
            .unwrap();
        let service = Arc::clone(&h.provider_service);
        let lease = service.acquire(id).await.unwrap();

        let check = tokio::spawn({
            let service = Arc::clone(&service);
            let lease = lease.clone();
            async move { service.check_quota(&lease).await }
        });
        arrived.notified().await;
        let replacement =
            (service.provider_factory)(&h.repository.get_provider(id).await.unwrap()).unwrap();
        service.replace(id, replacement, 30).await;
        release.notify_one();

        assert_eq!(check.await.unwrap(), None);
        assert_eq!(h.scheduler.paused_until(id, Utc::now()), None);
    }

    fn updated(h: &Harness, id: i64) -> ProviderConfig {
        let mut config = h.config("renamed", 45);
        config.id = Some(id);
        config
    }

    #[tokio::test]
    async fn an_update_during_a_blocked_fetch_supersedes_the_old_attempt() {
        let h = Harness::new().await;
        h.start().await;
        let old_gate = Gate::new();
        let (id, old) = h
            .add_stub(|id| {
                StubProvider::new(id)
                    .gated(&old_gate)
                    .returning(vec![pipeline(id, "old", PipelineStatus::Success)])
            })
            .await;
        let new = h.stubs.queue(StubProvider::new(id).returning(vec![pipeline(
            id,
            "new",
            PipelineStatus::Failed,
        )]));
        let blocked = h.spawn_fetch(id).await;
        old_gate.arrived().await;
        let before = h.provider_service.acquire(id).await.unwrap().generation;

        h.provider_service
            .update_provider(id, updated(&h, id))
            .await
            .unwrap();
        let updated_at = h
            .bus
            .names()
            .iter()
            .position(|name| *name == "provider-updated")
            .unwrap();
        h.bus
            .wait_for("the fetch queued by the update", |events| {
                events[updated_at..]
                    .iter()
                    .any(|event| matches!(event, CoreEvent::PipelinesUpdated { .. }))
            })
            .await;
        old_gate.open();
        let old_result = blocked.await.unwrap();

        assert!(matches!(old_result, FetchResult::Superseded));
        assert_eq!(h.cached_names(id).await, ["new"]);
        let published: Vec<_> = h.bus.events()[updated_at..]
            .iter()
            .filter_map(|event| match event {
                CoreEvent::PipelinesUpdated { pipelines, .. } => {
                    Some(pipelines.iter().map(|p| p.name.clone()).collect::<Vec<_>>())
                }
                _ => None,
            })
            .collect();
        assert_eq!(published, vec![vec!["new".to_string()]]);
        let view = h.scheduler.view(id, Instant::now()).unwrap();
        assert_eq!(
            (view.failures, view.last_failure, view.in_flight),
            (0, None, false)
        );
        assert_ne!(
            h.provider_service.acquire(id).await.unwrap().generation,
            before
        );
        assert_eq!((old.fetch_count(), new.fetch_count()), (1, 1));
    }

    #[tokio::test]
    async fn an_update_resets_the_fetch_status_and_the_schedule() {
        let h = Harness::new().await;
        let (id, _) = h
            .add_stub(|id| StubProvider::new(id).failing(DomainError::NetworkError("down".into())))
            .await;
        h.fetch_directly(id).await;
        h.health_inputs();
        assert_eq!(h.scheduler.view(id, Instant::now()).unwrap().failures, 1);

        h.provider_service
            .update_provider(id, updated(&h, id))
            .await
            .unwrap();
        let inputs = h.health_inputs();

        let event = h
            .bus
            .events()
            .into_iter()
            .find_map(|event| match event {
                CoreEvent::ProviderUpdated { provider, .. } => Some(provider),
                _ => None,
            })
            .unwrap();
        assert!(matches!(event.last_fetch_status, FetchStatus::Never));
        assert_eq!(event.last_fetch_error, None);
        assert_eq!(event.last_fetch_error_kind, None);
        assert_eq!(event.health.next_attempt_at, None);
        assert_eq!(event.name, "renamed");
        assert_eq!(h.scheduler.view(id, Instant::now()).unwrap().failures, 0);
        let lease = h.provider_service.acquire(id).await.unwrap();
        assert!(matches!(
            inputs.as_slice(),
            [HealthInput::ProvidersChanged { changed }]
                if changed.len() == 1 && changed[0].generation == Some(lease.generation)
        ));
    }

    #[tokio::test]
    async fn an_old_failure_finishing_after_an_update_leaves_the_new_schedule_clean() {
        let h = Harness::new().await;
        h.start().await;
        let old_gate = Gate::new();
        let new_gate = Gate::new();
        let (id, _) = h
            .add_stub(|id| {
                StubProvider::new(id)
                    .gated(&old_gate)
                    .failing(DomainError::NetworkError("down".into()))
            })
            .await;
        h.stubs.queue(StubProvider::new(id).gated(&new_gate));
        let blocked = h.spawn_fetch(id).await;
        old_gate.arrived().await;

        h.provider_service
            .update_provider(id, updated(&h, id))
            .await
            .unwrap();
        new_gate.arrived().await;
        old_gate.open();
        let old_result = blocked.await.unwrap();

        assert!(matches!(old_result, FetchResult::Superseded));
        let view = h.scheduler.view(id, Instant::now()).unwrap();
        assert_eq!(
            (view.failures, view.last_failure, view.in_flight),
            (0, None, true)
        );
        let (status, error, _) = h.repository.get_provider_fetch_status(id).await.unwrap();
        assert_eq!((status.as_str(), error), ("never", None));
        new_gate.open();
        h.bus
            .wait_for("the new fetch", |events| {
                events
                    .iter()
                    .any(|event| matches!(event, CoreEvent::PipelinesUpdated { .. }))
            })
            .await;
        assert_eq!(h.scheduler.view(id, Instant::now()).unwrap().failures, 0);
    }

    #[tokio::test]
    async fn a_removal_during_a_blocked_fetch_leaves_nothing_behind() {
        let h = Harness::new().await;
        let gate = Gate::new();
        let (id, _) = h.add_stub(|id| StubProvider::new(id).gated(&gate)).await;
        let blocked = h.spawn_fetch(id).await;
        gate.arrived().await;
        h.health_inputs();

        h.provider_service.remove_provider(id).await.unwrap();
        let removed_at = h.bus.events().len();
        gate.open();
        let result = blocked.await.unwrap();
        let inputs = h.health_inputs();

        assert!(matches!(result, FetchResult::Superseded));
        assert!(h.repository.get_provider(id).await.is_err());
        assert!(h.repository.get_provider_fetch_status(id).await.is_err());
        assert!(h.cached_names(id).await.is_empty());
        assert!(h.scheduler.view(id, Instant::now()).is_none());
        assert!(h.bus.names()[removed_at..].is_empty());
        assert_eq!(h.bus.names()[removed_at - 1], "provider-removed");
        assert!(matches!(
            inputs.as_slice(),
            [HealthInput::ProvidersChanged { changed }]
                if changed[0].id == id && changed[0].generation.is_none()
        ));
    }

    #[tokio::test]
    async fn concurrent_acquires_share_one_provider() {
        let h = Harness::new().await;
        let id = h.add_provider("shared").await;

        let leases =
            futures::future::join_all((0..8).map(|_| h.provider_service.acquire(id))).await;

        let first = leases[0].as_ref().unwrap();
        for lease in &leases {
            let lease = lease.as_ref().unwrap();
            assert!(Arc::ptr_eq(&lease.provider, &first.provider));
            assert_eq!(lease.generation, first.generation);
        }
    }

    #[tokio::test]
    async fn a_lazy_acquire_racing_a_removal_does_not_bring_the_provider_back() {
        let h = Harness::new().await;
        let id = h.add_provider("racing").await;
        let lock = h.provider_service.commit_lock(id);
        let held = lock.lock().await;
        let removal = tokio::spawn({
            let service = Arc::clone(&h.provider_service);
            async move { service.remove_provider(id).await }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;
        let acquire = tokio::spawn({
            let service = Arc::clone(&h.provider_service);
            async move { service.acquire(id).await }
        });
        tokio::time::sleep(Duration::from_millis(100)).await;

        drop(held);
        removal.await.unwrap().unwrap();
        let acquired = acquire.await.unwrap();

        assert!(matches!(acquired, Err(DomainError::ProviderNotFound(_))));
        assert!(h.provider_service.current(id).await.is_none());
        assert!(h.scheduler.view(id, Instant::now()).is_none());
    }

    #[tokio::test]
    async fn changing_the_refresh_interval_keeps_the_generation_and_reschedules() {
        let h = Harness::new().await;
        h.start().await;
        let (id, _) = h.add_stub(StubProvider::new).await;
        h.pipeline_service
            .fetch_now(&[id], Duration::from_secs(5))
            .await;
        let before = h.provider_service.acquire(id).await.unwrap().generation;
        assert!(
            h.scheduler
                .view(id, Instant::now())
                .unwrap()
                .next_attempt_in
                > Duration::from_secs(20)
        );

        h.provider_service
            .update_provider_refresh_interval(id, 5)
            .await
            .unwrap();

        let after = h.provider_service.acquire(id).await.unwrap().generation;
        assert_eq!(before, after);
        assert!(
            h.scheduler
                .view(id, Instant::now())
                .unwrap()
                .next_attempt_in
                <= Duration::from_secs(5)
        );
        assert_eq!(
            h.repository
                .get_provider(id)
                .await
                .unwrap()
                .refresh_interval,
            5
        );
    }

    #[tokio::test]
    async fn health_hears_about_every_provider_change() {
        let h = Harness::new().await;

        let id = h
            .provider_service
            .add_provider(h.config("argo", 30))
            .await
            .unwrap();
        let added = h.health_inputs();
        let added_generation = h.provider_service.acquire(id).await.unwrap().generation;
        h.provider_service
            .update_provider(id, updated(&h, id))
            .await
            .unwrap();
        let updated_inputs = h.health_inputs();
        h.provider_service.load_all_providers().await.unwrap();
        let reloaded = h.health_inputs();
        let reloaded_generation = h.provider_service.acquire(id).await.unwrap().generation;
        h.provider_service.remove_provider(id).await.unwrap();
        let removed = h.health_inputs();

        let generations = |inputs: &[HealthInput]| match inputs {
            [HealthInput::ProvidersChanged { changed }] => changed
                .iter()
                .map(|input| (input.id, input.generation))
                .collect::<Vec<_>>(),
            other => panic!("unexpected health inputs: {other:?}"),
        };
        assert_eq!(generations(&added), [(id, Some(added_generation))]);
        let updated_generation = generations(&updated_inputs)[0].1;
        assert!(updated_generation.is_some());
        assert_ne!(updated_generation, Some(added_generation));
        assert_eq!(generations(&reloaded), [(id, Some(reloaded_generation))]);
        assert_ne!(Some(reloaded_generation), updated_generation);
        assert_eq!(generations(&removed), [(id, None)]);
        assert_eq!(h.bus.count("provider-added"), 1);
        assert_eq!(h.bus.count("provider-updated"), 1);
        assert_eq!(h.bus.count("provider-removed"), 1);
    }

    #[tokio::test]
    async fn workflow_parameters_are_cached_for_ten_minutes() {
        let h = Harness::new().await;
        let (id, stub) = h.add_stub(StubProvider::new).await;
        let workflow = format!("github__{id}__web");

        let first = h
            .provider_service
            .get_workflow_parameters(id, &workflow)
            .await
            .unwrap();
        h.provider_service
            .get_workflow_parameters(id, &workflow)
            .await
            .unwrap();
        let cached_fetches = stub
            .parameter_fetches
            .load(std::sync::atomic::Ordering::SeqCst);
        sqlx::query(
            "UPDATE workflow_parameters_cache SET cached_at = datetime('now', '-11 minutes')",
        )
        .execute(&h.pool)
        .await
        .unwrap();
        h.provider_service
            .get_workflow_parameters(id, &workflow)
            .await
            .unwrap();

        assert_eq!(first.len(), 1);
        assert_eq!(cached_fetches, 1);
        assert_eq!(
            stub.parameter_fetches
                .load(std::sync::atomic::Ordering::SeqCst),
            2
        );
    }

    #[tokio::test]
    async fn workflow_parameter_failures_keep_their_kind_and_leave_no_lock_behind() {
        let h = Harness::new().await;
        let (id, stub) = h.add_stub(StubProvider::new).await;
        *stub.parameters.lock().unwrap() = Err(DomainError::RateLimited {
            message: "slow".to_string(),
            retry_after: None,
        });

        let failed = h
            .provider_service
            .get_workflow_parameters(id, "github__1__web")
            .await;
        let missing = h
            .provider_service
            .get_workflow_parameters(4242, "github__4242__web")
            .await;

        assert!(matches!(failed, Err(DomainError::RateLimited { .. })));
        assert!(matches!(missing, Err(DomainError::ProviderNotFound(_))));
        assert!(h.provider_service.parameter_fetches.is_empty());
    }

    #[tokio::test]
    async fn fetching_workflow_parameters_does_not_block_the_provider_map() {
        let h = Harness::new().await;
        let gate = Gate::new();
        let (id, _) = h
            .add_stub(|id| StubProvider::new(id).parameter_gated(&gate))
            .await;
        let other = h.add_provider("other").await;
        let fetch = tokio::spawn({
            let service = Arc::clone(&h.provider_service);
            async move { service.get_workflow_parameters(id, "github__1__web").await }
        });
        gate.arrived().await;

        let acquired =
            tokio::time::timeout(Duration::from_secs(2), h.provider_service.acquire(other)).await;
        gate.open();

        assert!(acquired.is_ok());
        fetch.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn workflow_parameters_fetched_across_a_removal_are_not_cached() {
        let h = Harness::new().await;
        let gate = Gate::new();
        let (id, _) = h
            .add_stub(|id| StubProvider::new(id).parameter_gated(&gate))
            .await;
        let workflow = format!("github__{id}__web");
        let fetch = tokio::spawn({
            let service = Arc::clone(&h.provider_service);
            let workflow = workflow.clone();
            async move { service.get_workflow_parameters(id, &workflow).await }
        });
        gate.arrived().await;

        h.provider_service.remove_provider(id).await.unwrap();
        gate.open();

        assert_eq!(fetch.await.unwrap().unwrap().len(), 1);
        assert!(h
            .repository
            .get_cached_workflow_parameters(&workflow)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn field_options_work_when_the_plugin_rejects_the_partial_form() {
        let h = Harness::with_real_plugins().await;
        let kubeconfig = h._temp_dir.path().join("kubeconfig");
        std::fs::write(
            &kubeconfig,
            "apiVersion: v1\nkind: Config\nclusters:\n- name: c\n  cluster: {server: 'https://127.0.0.1:6443'}\nusers:\n- name: u\n  user: {token: t}\ncontexts:\n- name: ctx-b\n  context: {cluster: c, user: u}\n- name: ctx-a\n  context: {cluster: c, user: u}\ncurrent-context: ctx-a\n",
        )
        .unwrap();
        let config = HashMap::from([
            ("kubeconfig".to_string(), kubeconfig.display().to_string()),
            ("namespaces".to_string(), "Not_A_Namespace".to_string()),
        ]);
        assert!(h
            .provider_service
            .init_plugin("fluxcd", config.clone(), 0)
            .is_err());

        let contexts = h
            .provider_service
            .field_options("fluxcd", "context", config)
            .await
            .unwrap();

        assert_eq!(contexts, ["ctx-a", "ctx-b"]);
    }

    #[tokio::test]
    async fn field_options_that_need_a_client_use_the_initialized_plugin() {
        let h = Harness::with_real_plugins().await;
        let ready = HashMap::from([
            ("server_url".to_string(), h.argocd_url.clone()),
            ("token".to_string(), "token".to_string()),
        ]);
        let incomplete = HashMap::from([("token".to_string(), "token".to_string())]);

        let organizations = h
            .provider_service
            .field_options("argocd", "organizations", ready)
            .await;
        let rejected = h
            .provider_service
            .field_options("argocd", "organizations", incomplete)
            .await;

        assert_eq!(organizations.unwrap(), Vec::<String>::new());
        assert!(
            matches!(rejected, Err(DomainError::InvalidConfig(message)) if message.contains("initialize"))
        );
    }

    fn serve_tls(stream: std::net::TcpStream, config: Arc<rustls::ServerConfig>) {
        let Ok(connection) = rustls::ServerConnection::new(config) else {
            return;
        };
        let mut tls = rustls::StreamOwned::new(connection, stream);
        let mut request = Vec::new();
        let mut buffer = [0; 4096];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            match tls.read(&mut buffer) {
                Ok(0) | Err(_) => return,
                Ok(count) => request.extend_from_slice(&buffer[..count]),
            }
        }
        let body = r#"{"items":[]}"#;
        let _ = write!(
            tls,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = tls.flush();
        tls.conn.send_close_notify();
        let _ = tls.flush();
    }

    fn self_signed_argocd() -> String {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let cert = rustls::pki_types::CertificateDer::from(
            include_bytes!("../../../tests/fixtures/self_signed_cert.der").to_vec(),
        );
        let key = rustls::pki_types::PrivateKeyDer::Pkcs8(
            include_bytes!("../../../tests/fixtures/self_signed_key.der")
                .to_vec()
                .into(),
        );
        let config = Arc::new(
            rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![cert], key)
                .unwrap(),
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let config = Arc::clone(&config);
                std::thread::spawn(move || serve_tls(stream, config));
            }
        });

        format!("https://{addr}")
    }

    fn argocd_settings(url: &str, insecure: bool) -> HashMap<String, String> {
        HashMap::from([
            ("server_url".to_string(), url.to_string()),
            ("token".to_string(), "token".to_string()),
            ("insecure".to_string(), insecure.to_string()),
        ])
    }

    #[tokio::test]
    async fn argocd_insecure_tls_is_honored_by_add_validate_and_preview() {
        let h = Harness::with_real_plugins().await;
        let service = &h.provider_service;
        let url = self_signed_argocd();
        let page = Some(pipedash_plugin_api::PaginationParams {
            page: 1,
            page_size: 10,
        });
        let add = |insecure| {
            let mut config = h.config("argo", 30);
            config.config = argocd_settings(&url, insecure);
            config.config.remove("token");
            service.add_provider(config)
        };

        let id = add(true).await.unwrap();
        let insecure = service
            .init_plugin("argocd", argocd_settings(&url, true), 0)
            .unwrap();
        let secure = service
            .init_plugin("argocd", argocd_settings(&url, false), 0)
            .unwrap();
        let rejected = add(false).await.unwrap_err();

        assert!(id > 0);
        assert!(insecure.validate_credentials().await.unwrap());
        assert!(insecure
            .fetch_available_pipelines_filtered(None, None, page.clone())
            .await
            .unwrap()
            .items
            .is_empty());
        assert!(matches!(
            secure.validate_credentials().await,
            Err(PluginError::Tls(_))
        ));
        assert!(matches!(
            secure
                .fetch_available_pipelines_filtered(None, None, page)
                .await,
            Err(PluginError::Tls(_))
        ));
        assert!(matches!(rejected, DomainError::InvalidConfig(message) if message.contains("TLS")));
    }

    fn config_file(h: &Harness, providers: &[(&str, &str, i64)]) {
        let mut content = String::new();
        for (key, name, refresh_interval) in providers {
            content.push_str(&format!(
                "[providers.{key}]\nname = \"{name}\"\ntype = \"argocd\"\ntoken = \"token-{key}\"\nrefresh_interval = {refresh_interval}\n\n[providers.{key}.config]\nserver_url = \"{}\"\n\n",
                h.argocd_url
            ));
        }
        std::fs::write(h.config_state.config_path(), content).unwrap();
    }

    async fn provider_id(h: &Harness, key: &str) -> Option<i64> {
        h.repository
            .config_backend()
            .list_providers()
            .await
            .unwrap()
            .into_iter()
            .find(|p| p.name == key)
            .and_then(|p| p.id)
    }

    #[tokio::test]
    async fn removal_preserves_the_provider_and_toml_when_token_deletion_fails() {
        let h = Harness::new().await;
        config_file(&h, &[("ci-a", "CI A", 30)]);
        h.provider_service
            .apply_config_file(&h.config_state)
            .await
            .unwrap();
        let id = provider_id(&h, "ci-a").await.unwrap();
        h.token_store.fail_next_delete();

        assert!(h.provider_service.remove_provider(id).await.is_err());
        assert!(h.repository.get_provider(id).await.is_ok());
        assert!(h
            .config_state
            .load_providers()
            .unwrap()
            .contains_key("ci-a"));

        h.provider_service.remove_provider(id).await.unwrap();
        h.config_state.reload().await.unwrap();
        h.config_state
            .sync_providers_from_toml(h.repository.config_backend().as_ref(), false)
            .await
            .unwrap();
        assert!(h.repository.list_providers().await.unwrap().is_empty());
        assert!(h.token_store.get_all_tokens().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn removal_awaits_token_cleanup_before_deleting_the_provider() {
        let h = Harness::new().await;
        let id = h.add_provider("await-delete").await;
        let gate = Gate::new();
        h.token_store.gate_delete(Arc::clone(&gate));
        let remove = tokio::spawn({
            let service = Arc::clone(&h.provider_service);
            async move { service.remove_provider(id).await }
        });
        gate.arrived().await;

        assert!(!remove.is_finished());
        assert!(h.repository.get_provider(id).await.is_ok());

        gate.open();
        remove.await.unwrap().unwrap();
        assert!(h.repository.get_provider(id).await.is_err());
        assert!(h.token_store.get_all_tokens().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn failed_config_deletion_keeps_the_provider_available_for_retry() {
        let h = Harness::new().await;
        config_file(&h, &[("ci-a", "CI A", 30)]);
        h.provider_service
            .apply_config_file(&h.config_state)
            .await
            .unwrap();
        let id = provider_id(&h, "ci-a").await.unwrap();
        let path = h.config_state.config_path();
        std::fs::remove_file(path).unwrap();
        std::fs::create_dir(path).unwrap();

        assert!(h.provider_service.remove_provider(id).await.is_err());
        assert!(h.repository.get_provider(id).await.is_ok());

        std::fs::remove_dir(path).unwrap();
        config_file(&h, &[("ci-a", "CI A", 30)]);
        h.provider_service.remove_provider(id).await.unwrap();
        assert!(h.repository.get_provider(id).await.is_err());
        assert!(h.config_state.load_providers().unwrap().is_empty());
    }

    #[tokio::test]
    async fn renaming_a_toml_provider_keeps_the_file_key_for_removal() {
        let h = Harness::new().await;
        config_file(&h, &[("ci-a", "CI A", 30)]);
        h.provider_service
            .apply_config_file(&h.config_state)
            .await
            .unwrap();
        let id = provider_id(&h, "ci-a").await.unwrap();
        let mut renamed = h.repository.get_provider(id).await.unwrap();
        renamed.name = "Renamed CI".to_string();

        h.provider_service
            .update_provider(id, renamed)
            .await
            .unwrap();
        let saved = h.repository.get_provider(id).await.unwrap();
        assert_eq!(
            (saved.name.as_str(), saved.display_name()),
            ("ci-a", "Renamed CI")
        );

        h.provider_service.remove_provider(id).await.unwrap();
        h.config_state.reload().await.unwrap();
        h.config_state
            .sync_providers_from_toml(h.repository.config_backend().as_ref(), false)
            .await
            .unwrap();
        assert!(h.repository.list_providers().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_apply_config_file_adds_and_updates_changed_blocks_only() {
        let h = Harness::new().await;
        config_file(&h, &[("ci-a", "CI A", 30), ("ci-b", "CI B", 30)]);

        let first = h
            .provider_service
            .apply_config_file(&h.config_state)
            .await
            .unwrap();

        assert_eq!(first.added, ["ci-a", "ci-b"]);
        let a = provider_id(&h, "ci-a").await.unwrap();
        let b = provider_id(&h, "ci-b").await.unwrap();
        let loaded = h.provider_service.generation_of(a).await.unwrap();
        assert!(h.provider_service.generation_of(b).await.is_some());
        assert!(h
            .bus
            .events()
            .iter()
            .any(|e| matches!(e, CoreEvent::ProvidersChanged)));

        h.provider_service
            .update_provider_refresh_interval(b, 120)
            .await
            .unwrap();
        config_file(
            &h,
            &[
                ("ci-a", "CI Renamed", 30),
                ("ci-b", "CI B", 30),
                ("ci-c", "CI C", 60),
            ],
        );

        let second = h
            .provider_service
            .apply_config_file(&h.config_state)
            .await
            .unwrap();

        assert_eq!(second.added, ["ci-c"]);
        assert_eq!(second.updated, ["ci-a"]);
        let c = provider_id(&h, "ci-c").await.unwrap();
        assert_eq!(
            h.repository.get_provider(c).await.unwrap().token,
            "token-ci-c"
        );
        assert!(h.provider_service.generation_of(c).await.is_some());
        assert_eq!(
            h.repository.get_provider(a).await.unwrap().display_name(),
            "CI Renamed"
        );
        assert_ne!(h.provider_service.generation_of(a).await, Some(loaded));
        assert_eq!(
            h.repository.get_provider(b).await.unwrap().refresh_interval,
            120
        );

        let unchanged = h
            .provider_service
            .apply_config_file(&h.config_state)
            .await
            .unwrap();
        assert!(unchanged.added.is_empty() && unchanged.updated.is_empty());
    }

    #[tokio::test]
    async fn test_apply_config_file_rejects_an_invalid_file_and_keeps_providers() {
        let h = Harness::new().await;
        config_file(&h, &[("ci-a", "CI A", 30)]);
        h.provider_service
            .apply_config_file(&h.config_state)
            .await
            .unwrap();
        let a = provider_id(&h, "ci-a").await.unwrap();

        let write = |content: &str| std::fs::write(h.config_state.config_path(), content).unwrap();

        write("[providers.ci-a]\nname = \"CI Broken\"\ntype = \"\"\n");
        let empty = h.provider_service.apply_config_file(&h.config_state).await;
        write("[providers.ci-a]\nname = \"CI Broken\"\ntype = \"circleci\"\n");
        let unknown = h.provider_service.apply_config_file(&h.config_state).await;

        assert!(matches!(empty, Err(DomainError::InvalidConfig(_))));
        assert!(matches!(unknown, Err(DomainError::InvalidProviderType(_))));

        assert_eq!(
            h.repository.get_provider(a).await.unwrap().display_name(),
            "CI A"
        );
        assert_eq!(
            h.config_state.get().await.providers["ci-a"].provider_type,
            "argocd"
        );
    }

    async fn execute(h: &Harness, sql: &'static str) {
        sqlx::query(sql).execute(&h.pool).await.unwrap();
    }

    #[tokio::test]
    async fn test_apply_config_file_retry_replaces_providers_saved_by_a_failed_apply() {
        let h = Harness::new().await;
        config_file(&h, &[("ci-a", "CI A", 30), ("ci-b", "CI B", 30)]);
        h.provider_service
            .apply_config_file(&h.config_state)
            .await
            .unwrap();
        let a = provider_id(&h, "ci-a").await.unwrap();
        let b = provider_id(&h, "ci-b").await.unwrap();
        let loaded_a = h.provider_service.generation_of(a).await.unwrap();
        let loaded_b = h.provider_service.generation_of(b).await.unwrap();

        execute(
            &h,
            "CREATE TRIGGER fail_ci_b BEFORE UPDATE ON providers WHEN NEW.name = 'ci-b' \
             BEGIN SELECT RAISE(ABORT, 'write failed'); END",
        )
        .await;
        config_file(&h, &[("ci-a", "CI A2", 30), ("ci-b", "CI B2", 30)]);
        assert!(h
            .provider_service
            .apply_config_file(&h.config_state)
            .await
            .is_err());
        assert_eq!(
            h.repository.get_provider(a).await.unwrap().display_name(),
            "CI A2"
        );

        execute(&h, "DROP TRIGGER fail_ci_b").await;
        h.provider_service
            .apply_config_file(&h.config_state)
            .await
            .unwrap();

        assert_eq!(
            h.repository.get_provider(b).await.unwrap().display_name(),
            "CI B2"
        );
        assert_ne!(h.provider_service.generation_of(a).await, Some(loaded_a));
        assert_ne!(h.provider_service.generation_of(b).await, Some(loaded_b));
        assert_eq!(
            h.config_state.get().await.providers["ci-b"].name.as_deref(),
            Some("CI B2")
        );
    }

    #[tokio::test]
    async fn test_apply_config_file_stores_an_empty_token_for_a_new_provider_without_one() {
        let h = Harness::new().await;
        config_file(&h, &[("ci-a", "CI A", 30)]);
        let path = h.config_state.config_path();
        let unset = std::fs::read_to_string(path)
            .unwrap()
            .replace("token-ci-a", "${PIPEDASH_TOKEN_UNSET_IN_TESTS:-}");
        std::fs::write(path, unset).unwrap();

        h.provider_service
            .apply_config_file(&h.config_state)
            .await
            .unwrap();

        let a = provider_id(&h, "ci-a").await.unwrap();
        assert_eq!(h.token_store.get_token(a).await.unwrap(), "");
    }

    #[tokio::test]
    async fn test_apply_config_file_retries_a_token_the_store_failed_to_save() {
        let h = Harness::new().await;
        config_file(&h, &[("ci-a", "CI A", 30)]);
        h.provider_service
            .apply_config_file(&h.config_state)
            .await
            .unwrap();
        let a = provider_id(&h, "ci-a").await.unwrap();
        let path = h.config_state.config_path();
        let rotated = std::fs::read_to_string(path)
            .unwrap()
            .replace("token-ci-a", "rotated");
        std::fs::write(path, rotated).unwrap();

        h.token_store.fail_next_store();
        assert!(h
            .provider_service
            .apply_config_file(&h.config_state)
            .await
            .is_err());
        assert_eq!(h.stubs.token_of(a).as_deref(), Some("token-ci-a"));

        h.provider_service
            .apply_config_file(&h.config_state)
            .await
            .unwrap();

        assert_eq!(h.stubs.token_of(a).as_deref(), Some("rotated"));
        assert_eq!(
            h.config_state.get().await.providers["ci-a"].token,
            "rotated"
        );
    }

    #[tokio::test]
    async fn test_config_watcher_retries_a_failed_apply_without_a_new_edit() {
        let h = Harness::new().await;
        let watcher = crate::application::ConfigWatcher::new(
            Arc::clone(&h.provider_service),
            Arc::clone(&h.config_state),
        )
        .with_poll(Duration::from_millis(20));
        watcher.start().await;

        execute(
            &h,
            "CREATE TRIGGER fail_insert BEFORE INSERT ON providers \
             BEGIN SELECT RAISE(ABORT, 'write failed'); END",
        )
        .await;
        config_file(&h, &[("ci-a", "CI A", 30)]);
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(provider_id(&h, "ci-a").await, None);
        execute(&h, "DROP TRIGGER fail_insert").await;

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let id = loop {
            if let Some(id) = provider_id(&h, "ci-a").await {
                break id;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the watcher never retried the file"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        watcher.stop().await;

        assert!(h.provider_service.generation_of(id).await.is_some());
    }
}
