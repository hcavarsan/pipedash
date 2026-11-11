use std::collections::HashMap;
use std::sync::{
    Arc,
    OnceLock,
};

use async_trait::async_trait;
use dashmap::DashMap;
use pipedash_plugin_api::PluginRegistry;
use plugin::plugin_provider;
use tokio::sync::{
    Mutex,
    RwLock,
};

use crate::application::fetch_scheduler::{
    FetchScheduler,
    Generation,
};
use crate::application::refresh_manager::RefreshDispatcher;
use crate::application::services::health_service::HealthHandle;
use crate::application::services::sign_in_service::SignInService;
use crate::domain::{
    DomainResult,
    Provider,
    ProviderConfig,
};
use crate::event::EventBus;
use crate::infrastructure::config::{
    ConfigState,
    PipedashConfig,
};
use crate::infrastructure::database::Repository;
use crate::infrastructure::run_logs::LogStore;
use crate::infrastructure::HttpClientManager;
mod attention;
mod auth;
mod capabilities;
mod config;
mod discovery;
mod draft;
pub mod dto;
mod mutations;
mod parameters;
pub(crate) mod plugin;
mod preferences;
pub(crate) mod quota;
mod reconnect;
mod runtime;
mod summary;
#[cfg(test)]
mod tests;

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

#[async_trait]
pub(crate) trait ApplicationsConfig: Send + Sync {
    async fn check(&self, loaded: &PipedashConfig) -> DomainResult<()>;
    async fn commit(&self, loaded: &PipedashConfig) -> DomainResult<()>;
    async fn failed(&self, message: String);
}

pub struct ProviderService {
    repository: Arc<Repository>,
    config_state: Arc<ConfigState>,
    http_client_manager: Arc<HttpClientManager>,
    providers: RwLock<HashMap<i64, ProviderSlot>>,
    commit_locks: DashMap<i64, Arc<Mutex<()>>>,
    renewal_locks: DashMap<i64, Arc<Mutex<()>>>,
    sign_ins: Arc<SignInService>,
    plugin_registry: Arc<PluginRegistry>,
    parameter_fetches: DashMap<String, Arc<Mutex<()>>>,
    discovery: discovery::DiscoveryCache,
    event_bus: Arc<dyn EventBus>,
    scheduler: Arc<FetchScheduler>,
    health: HealthHandle,
    dispatcher: RefreshDispatcher,
    provider_factory: ProviderFactory,
    config_apply: Mutex<()>,
    applications: OnceLock<Arc<dyn ApplicationsConfig>>,
    log_store: Arc<LogStore>,
}

pub struct ProviderDeps {
    pub repository: Arc<Repository>,
    pub http_client_manager: Arc<HttpClientManager>,
    pub config_state: Arc<ConfigState>,
    pub log_store: Arc<LogStore>,
    pub plugin_registry: Arc<PluginRegistry>,
}

impl ProviderService {
    pub fn new(
        deps: ProviderDeps, event_bus: Arc<dyn EventBus>, scheduler: Arc<FetchScheduler>,
        health: HealthHandle, dispatcher: RefreshDispatcher,
    ) -> Self {
        let ProviderDeps {
            repository,
            http_client_manager,
            config_state,
            log_store,
            plugin_registry,
        } = deps;
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
            renewal_locks: DashMap::new(),
            sign_ins: Arc::new(SignInService::default()),
            plugin_registry,
            parameter_fetches: DashMap::new(),
            discovery: discovery::DiscoveryCache::default(),
            event_bus,
            scheduler,
            health,
            dispatcher,
            provider_factory,
            config_apply: Mutex::new(()),
            applications: OnceLock::new(),
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

    pub(crate) fn plugin_registry(&self) -> &Arc<PluginRegistry> {
        &self.plugin_registry
    }

    pub(crate) fn set_applications(&self, applications: Arc<dyn ApplicationsConfig>) {
        let _ = self.applications.set(applications);
    }

    pub async fn config_failed(&self, message: String) {
        if let Some(applications) = self.applications.get() {
            applications.failed(message).await;
        }
    }
}
