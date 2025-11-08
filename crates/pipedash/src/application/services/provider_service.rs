use std::collections::HashMap;
use std::sync::Arc;

use pipedash_plugin_api::{
    Plugin as PluginTrait,
    PluginRegistry,
};
use tokio::sync::{
    Mutex,
    RwLock,
};

use crate::domain::{
    DomainResult,
    Provider,
    ProviderConfig,
    ProviderSummary,
};
use crate::infrastructure::{
    database::Repository,
    providers::PluginAdapter,
};

pub struct ProviderService {
    repository: Arc<Repository>,
    providers: Arc<RwLock<HashMap<i64, Arc<dyn Provider>>>>,
    plugin_registry: Arc<PluginRegistry>,
    parameter_fetches: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
}

impl ProviderService {
    pub fn new(repository: Arc<Repository>) -> Self {
        let plugin_registry = crate::plugins::init_registry();

        Self {
            repository,
            providers: Arc::new(RwLock::new(HashMap::new())),
            plugin_registry: Arc::new(plugin_registry),
            parameter_fetches: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn repository(&self) -> &Arc<Repository> {
        &self.repository
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

    pub fn create_uninitialized_plugin(
        &self, provider_type: &str,
    ) -> DomainResult<Box<dyn PluginTrait>> {
        crate::plugins::create_instance(provider_type).ok_or_else(|| {
            crate::domain::DomainError::InvalidProviderType(format!(
                "Unknown provider type: {}",
                provider_type
            ))
        })
    }

    pub async fn add_provider(&self, config: ProviderConfig) -> DomainResult<i64> {
        let provider = self.create_provider(&config)?;

        provider.validate_credentials().await?;

        let provider_type = provider.provider_type();
        if provider_type != config.provider_type {
            return Err(crate::domain::DomainError::InvalidConfig(format!(
                "Provider type mismatch: expected '{}', got '{}'",
                config.provider_type, provider_type
            )));
        }

        let id = self.repository.add_provider(&config).await?;

        let mut config_with_id = config.clone();
        config_with_id.id = Some(id);
        let provider = self.create_provider(&config_with_id)?;

        let mut providers = self.providers.write().await;
        providers.insert(id, provider);

        Ok(id)
    }

    pub async fn get_provider_config(&self, id: i64) -> DomainResult<ProviderConfig> {
        self.repository.get_provider(id).await
    }

    pub async fn list_providers(&self) -> DomainResult<Vec<ProviderSummary>> {
        let configs = self.repository.list_providers().await?;
        let mut summaries = Vec::new();

        for config in configs {
            let cached_pipelines = self.repository.get_cached_pipelines(config.id).await?;
            let pipeline_count = cached_pipelines.len();
            let last_updated = cached_pipelines.iter().map(|p| p.last_updated).max();

            let icon = self
                .plugin_registry
                .get(&config.provider_type)
                .and_then(|plugin| plugin.metadata().icon.clone());

            let configured_repositories = config
                .config
                .get("selected_items")
                .map(|items| {
                    if config.provider_type == "argocd" {
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
                .unwrap_or_default();

            summaries.push(ProviderSummary {
                id: config.id.unwrap(),
                name: config.name,
                provider_type: config.provider_type,
                icon,
                pipeline_count,
                last_updated,
                refresh_interval: config.refresh_interval,
                configured_repositories,
            });
        }

        Ok(summaries)
    }

    pub async fn update_provider(&self, id: i64, config: ProviderConfig) -> DomainResult<()> {
        // Validate the new configuration
        let provider = self.create_provider(&config)?;
        provider.validate_credentials().await?;

        // Update in database
        self.repository.update_provider(id, &config).await?;

        // Update in memory
        let mut config_with_id = config.clone();
        config_with_id.id = Some(id);
        let new_provider = self.create_provider(&config_with_id)?;

        let mut providers = self.providers.write().await;
        providers.insert(id, new_provider);

        Ok(())
    }

    pub async fn update_provider_refresh_interval(
        &self, id: i64, refresh_interval: i64,
    ) -> DomainResult<()> {
        let mut config = self.repository.get_provider(id).await?;

        config.refresh_interval = refresh_interval;

        self.repository.update_provider(id, &config).await?;

        Ok(())
    }

    pub async fn remove_provider(&self, id: i64) -> DomainResult<()> {
        let pipelines = self.repository.get_cached_pipelines(Some(id)).await?;

        self.repository.remove_provider(id).await?;

        let mut providers = self.providers.write().await;
        providers.remove(&id);

        let mut fetches = self.parameter_fetches.lock().await;
        for pipeline in pipelines {
            fetches.remove(&pipeline.id);
        }

        Ok(())
    }

    pub async fn get_provider(&self, id: i64) -> DomainResult<Arc<dyn Provider>> {
        let providers = self.providers.read().await;

        if let Some(provider) = providers.get(&id) {
            return Ok(Arc::clone(provider));
        }

        drop(providers);

        let config = self.repository.get_provider(id).await?;
        let provider = self.create_provider(&config)?;

        let mut providers = self.providers.write().await;
        providers.insert(id, Arc::clone(&provider));

        Ok(provider)
    }

    pub async fn load_all_providers(&self) -> DomainResult<()> {
        let configs = self.repository.list_providers().await?;

        for config in configs {
            if let Some(id) = config.id {
                let provider = self.create_provider(&config)?;
                let mut providers = self.providers.write().await;
                providers.insert(id, provider);
            }
        }

        Ok(())
    }

    fn create_provider(&self, config: &ProviderConfig) -> DomainResult<Arc<dyn Provider>> {
        if !self.plugin_registry.is_registered(&config.provider_type) {
            return Err(crate::domain::DomainError::InvalidProviderType(format!(
                "Plugin not found for provider type: {}",
                config.provider_type
            )));
        }

        let mut plugin = self.create_uninitialized_plugin(&config.provider_type)?;

        let provider_id = config.id.unwrap_or(0);
        let mut plugin_config = config.config.clone();

        eprintln!(
            "[PROVIDER_SERVICE] Provider {} token length: {}",
            provider_id,
            config.token.len()
        );

        plugin_config.insert("token".to_string(), config.token.clone());

        plugin.initialize(provider_id, plugin_config).map_err(|e| {
            crate::domain::DomainError::InvalidConfig(format!("Failed to initialize plugin: {e}"))
        })?;

        let adapter = PluginAdapter::new(plugin);
        Ok(Arc::new(adapter))
    }

    pub async fn get_workflow_parameters(
        &self, provider_id: i64, workflow_id: &str,
    ) -> DomainResult<Vec<pipedash_plugin_api::WorkflowParameter>> {
        let start = std::time::Instant::now();

        if let Ok(Some(cached)) = self
            .repository
            .get_cached_workflow_parameters(workflow_id)
            .await
        {
            eprintln!(
                "[PERF] Got workflow parameters from cache for {} in {:?}",
                workflow_id,
                start.elapsed()
            );
            return Ok(cached);
        }

        eprintln!("[PERF] Cache miss for workflow parameters {workflow_id}, fetching from API");

        let fetch_lock = {
            let mut fetches = self.parameter_fetches.lock().await;
            fetches
                .entry(workflow_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };

        let _guard = fetch_lock.lock().await;
        eprintln!("[PERF] Acquired fetch lock for {workflow_id}");

        if let Ok(Some(cached)) = self
            .repository
            .get_cached_workflow_parameters(workflow_id)
            .await
        {
            eprintln!(
                "[PERF] Got workflow parameters from cache (after lock) for {} in {:?}",
                workflow_id,
                start.elapsed()
            );
            return Ok(cached);
        }

        let fetch_start = std::time::Instant::now();

        let providers = self.providers.read().await;
        let provider = providers.get(&provider_id).ok_or_else(|| {
            crate::domain::DomainError::ProviderNotFound(format!(
                "Provider {provider_id} not found"
            ))
        })?;

        let parameters = provider
            .get_workflow_parameters(workflow_id)
            .await
            .map_err(|e| {
                crate::domain::DomainError::InvalidConfig(format!(
                    "Failed to get workflow parameters: {e}"
                ))
            })?;

        eprintln!(
            "[PERF] Fetched {} parameters from API in {:?}",
            parameters.len(),
            fetch_start.elapsed()
        );

        if let Err(e) = self
            .repository
            .cache_workflow_parameters(workflow_id, &parameters)
            .await
        {
            eprintln!("[WARN] Failed to cache workflow parameters: {e}");
        }

        {
            let mut fetches = self.parameter_fetches.lock().await;
            fetches.remove(workflow_id);
        }

        eprintln!(
            "[PERF] Total get_workflow_parameters time: {:?}",
            start.elapsed()
        );
        Ok(parameters)
    }
}
