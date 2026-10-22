use std::collections::HashMap;
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
};
use tokio::sync::{
    Mutex,
    RwLock,
};
use tokio::time::timeout;

use crate::domain::{
    DomainError,
    DomainResult,
    FetchStatus,
    Provider,
    ProviderConfig,
    ProviderSummary,
};
use crate::event::{
    CoreEvent,
    EventBus,
};
use crate::infrastructure::config::sync::ProviderSyncService;
use crate::infrastructure::config::token_ref::TokenReference;
use crate::infrastructure::database::Repository;
use crate::infrastructure::providers::PluginAdapter;
use crate::plugins;

const QUOTA_PAUSE_PERCENT: u64 = 10;

pub struct ProviderService {
    repository: Arc<Repository>,
    http_client_manager: Arc<crate::infrastructure::HttpClientManager>,
    providers: Arc<RwLock<HashMap<i64, Arc<dyn Provider>>>>,
    plugin_registry: Arc<PluginRegistry>,
    parameter_fetches: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    event_bus: Arc<dyn EventBus>,
    quota_pauses: Arc<DashMap<i64, DateTime<Utc>>>,
}

impl ProviderService {
    pub fn new(
        repository: Arc<Repository>,
        http_client_manager: Arc<crate::infrastructure::HttpClientManager>,
        event_bus: Arc<dyn EventBus>,
    ) -> Self {
        let plugin_registry = plugins::create_plugin_registry();

        Self {
            repository,
            http_client_manager,
            providers: Arc::new(RwLock::new(HashMap::new())),
            plugin_registry: Arc::new(plugin_registry),
            parameter_fetches: Arc::new(Mutex::new(HashMap::new())),
            event_bus,
            quota_pauses: Arc::new(DashMap::new()),
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
        plugins::create_plugin(provider_type).ok_or_else(|| {
            DomainError::InvalidProviderType(format!("Unknown provider type: {}", provider_type))
        })
    }

    pub async fn add_provider(&self, config: ProviderConfig) -> DomainResult<i64> {
        let mut plugin = self.create_uninitialized_plugin(&config.provider_type)?;

        let mut plugin_config = config.config.clone();
        plugin_config.insert("token".to_string(), config.token.clone());

        let http_client = if let Some(base_url) = plugin_config.get("base_url") {
            self.http_client_manager.client_for_url(base_url)?
        } else {
            self.http_client_manager.default_client()
        };

        plugin
            .initialize(0, plugin_config.clone(), Some(http_client))
            .map_err(|e| DomainError::InvalidConfig(format!("Failed to initialize plugin: {e}")))?;

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

        let permission_status = permission_status.ok();

        let id = self.repository.add_provider(&config).await?;

        if let Some(ref status) = permission_status {
            if let Err(e) = self.repository.store_provider_permissions(id, status).await {
                tracing::warn!(provider_id = id, error = %e, "Failed to store permissions");
            }
        }

        let mut config_with_id = config.clone();
        config_with_id.id = Some(id);
        let provider = self.create_provider(&config_with_id)?;

        let mut providers = self.providers.write().await;
        providers.insert(id, provider);
        drop(providers);

        let fresh_config = self.repository.get_provider(id).await?;
        let cached_pipelines = self.repository.get_cached_pipelines(Some(id)).await?;
        let pipeline_count = cached_pipelines.len();
        let last_updated = cached_pipelines.iter().map(|p| p.last_updated).max();

        let icon = self
            .plugin_registry
            .get(&fresh_config.provider_type)
            .and_then(|plugin| plugin.metadata().icon.clone());

        let configured_repositories = fresh_config
            .config
            .get("selected_items")
            .map(|items| {
                if matches!(fresh_config.provider_type.as_str(), "argocd" | "fluxcd") {
                    return Vec::new();
                }

                items
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .map(|item| {
                        if fresh_config.provider_type == "tekton" && item.contains("__") {
                            item.replace("__", "/")
                        } else if fresh_config.provider_type == "jenkins" && !item.contains('/') {
                            format!("(root)/{}", item)
                        } else {
                            item
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        const FETCH_STATUS_SUCCESS: &str = "success";
        const FETCH_STATUS_ERROR: &str = "error";
        const FETCH_STATUS_NEVER: &str = "never";

        let (fetch_status_str, last_fetch_error, last_fetch_at) = self
            .repository
            .get_provider_fetch_status(id)
            .await
            .unwrap_or_else(|_| (FETCH_STATUS_NEVER.to_string(), None, None));

        let fetch_status_enum = match fetch_status_str.as_str() {
            FETCH_STATUS_SUCCESS => FetchStatus::Success,
            FETCH_STATUS_ERROR => FetchStatus::Error,
            _ => FetchStatus::Never,
        };

        let last_fetch_at_parsed = last_fetch_at.and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        });

        let provider_summary = ProviderSummary {
            id,
            name: fresh_config.display_name().to_string(),
            provider_type: fresh_config.provider_type,
            icon,
            pipeline_count,
            last_updated,
            refresh_interval: fresh_config.refresh_interval,
            configured_repositories,
            last_fetch_status: fetch_status_enum,
            last_fetch_error,
            last_fetch_at: last_fetch_at_parsed,
            paused_until: self.paused_until(id),
            version: fresh_config.version.unwrap_or(1),
        };

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        self.event_bus
            .emit(CoreEvent::ProviderAdded {
                provider: provider_summary,
                timestamp,
            })
            .await;

        Ok(id)
    }

    pub async fn export_providers_toml(&self) -> DomainResult<String> {
        let providers = self.repository.config_backend().list_providers().await?;

        ProviderSyncService::export_toml(&providers)
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
                .unwrap_or_default();

            let provider_id = config.id.unwrap();
            const FETCH_STATUS_SUCCESS: &str = "success";
            const FETCH_STATUS_ERROR: &str = "error";
            const FETCH_STATUS_NEVER: &str = "never";

            let (fetch_status, last_fetch_error, last_fetch_at) = self
                .repository
                .get_provider_fetch_status(provider_id)
                .await
                .unwrap_or_else(|_| (FETCH_STATUS_NEVER.to_string(), None, None));

            let fetch_status_enum = match fetch_status.as_str() {
                FETCH_STATUS_SUCCESS => FetchStatus::Success,
                FETCH_STATUS_ERROR => FetchStatus::Error,
                _ => FetchStatus::Never,
            };

            let last_fetch_at_parsed = last_fetch_at.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .ok()
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            });

            summaries.push(ProviderSummary {
                id: provider_id,
                name: config.display_name().to_string(),
                provider_type: config.provider_type,
                icon,
                pipeline_count,
                last_updated,
                refresh_interval: config.refresh_interval,
                configured_repositories,
                last_fetch_status: fetch_status_enum,
                last_fetch_error,
                last_fetch_at: last_fetch_at_parsed,
                paused_until: self.paused_until(provider_id),
                version: config.version.unwrap_or(1),
            });
        }

        Ok(summaries)
    }

    pub async fn update_provider(&self, id: i64, config: ProviderConfig) -> DomainResult<()> {
        let mut plugin = self.create_uninitialized_plugin(&config.provider_type)?;

        let mut plugin_config = config.config.clone();
        plugin_config.insert("token".to_string(), config.token.clone());

        let http_client = if let Some(base_url) = plugin_config.get("base_url") {
            self.http_client_manager.client_for_url(base_url)?
        } else {
            self.http_client_manager.default_client()
        };

        plugin
            .initialize(id, plugin_config.clone(), Some(http_client))
            .map_err(|e| DomainError::InvalidConfig(format!("Failed to initialize plugin: {e}")))?;

        plugin.validate_credentials().await.map_err(|e| {
            DomainError::InvalidConfig(format!("Credential validation failed: {e}"))
        })?;

        let permission_status = plugin.check_permissions().await.ok();

        let current = self.repository.get_provider(id).await?;
        let current_version = current.version.unwrap_or(1);

        let success = self
            .repository
            .update_provider_with_version(id, &config, current_version)
            .await?;

        if !success {
            return Err(DomainError::ConcurrentModification(format!(
                "Provider {} was modified by another client",
                id
            )));
        }

        if let Some(ref status) = permission_status {
            if let Err(e) = self.repository.store_provider_permissions(id, status).await {
                tracing::warn!(provider_id = id, error = %e, "Failed to store permissions");
            }
        }

        let mut config_with_id = config.clone();
        config_with_id.id = Some(id);
        let new_provider = self.create_provider(&config_with_id)?;

        let providers = Arc::clone(&self.providers);
        let old_provider = {
            let mut map = providers.write().await;
            map.insert(id, new_provider)
        };
        self.quota_pauses.remove(&id);

        if old_provider.is_some() {
            tokio::spawn(async move {
                drop(old_provider);
                tracing::debug!(
                    provider_id = id,
                    "Old provider cleanup completed after update"
                );
            });
        }

        let fresh_config = self.repository.get_provider(id).await?;
        let cached_pipelines = self.repository.get_cached_pipelines(Some(id)).await?;
        let pipeline_count = cached_pipelines.len();
        let last_updated = cached_pipelines.iter().map(|p| p.last_updated).max();

        let icon = self
            .plugin_registry
            .get(&fresh_config.provider_type)
            .and_then(|plugin| plugin.metadata().icon.clone());

        let configured_repositories = fresh_config
            .config
            .get("selected_items")
            .map(|items| {
                if matches!(fresh_config.provider_type.as_str(), "argocd" | "fluxcd") {
                    return Vec::new();
                }

                items
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .map(|item| {
                        if fresh_config.provider_type == "tekton" && item.contains("__") {
                            item.replace("__", "/")
                        } else if fresh_config.provider_type == "jenkins" && !item.contains('/') {
                            format!("(root)/{}", item)
                        } else {
                            item
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        const FETCH_STATUS_SUCCESS: &str = "success";
        const FETCH_STATUS_ERROR: &str = "error";
        const FETCH_STATUS_NEVER: &str = "never";

        let (fetch_status_str, last_fetch_error, last_fetch_at) = self
            .repository
            .get_provider_fetch_status(id)
            .await
            .unwrap_or_else(|_| (FETCH_STATUS_NEVER.to_string(), None, None));

        let fetch_status_enum = match fetch_status_str.as_str() {
            FETCH_STATUS_SUCCESS => FetchStatus::Success,
            FETCH_STATUS_ERROR => FetchStatus::Error,
            _ => FetchStatus::Never,
        };

        let last_fetch_at_parsed = last_fetch_at.and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        });

        let provider_summary = ProviderSummary {
            id,
            name: fresh_config.display_name().to_string(),
            provider_type: fresh_config.provider_type,
            icon,
            pipeline_count,
            last_updated,
            refresh_interval: fresh_config.refresh_interval,
            configured_repositories,
            last_fetch_status: fetch_status_enum,
            last_fetch_error,
            last_fetch_at: last_fetch_at_parsed,
            paused_until: self.paused_until(id),
            version: fresh_config.version.unwrap_or(1),
        };

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        self.event_bus
            .emit(CoreEvent::ProviderUpdated {
                provider: provider_summary,
                timestamp,
            })
            .await;

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
        let fresh_config = self.repository.get_provider(id).await?;
        let cached_pipelines = self.repository.get_cached_pipelines(Some(id)).await?;
        let pipeline_count = cached_pipelines.len();
        let last_updated = cached_pipelines.iter().map(|p| p.last_updated).max();

        let icon = self
            .plugin_registry
            .get(&fresh_config.provider_type)
            .and_then(|plugin| plugin.metadata().icon.clone());

        let configured_repositories = fresh_config
            .config
            .get("selected_items")
            .map(|items| {
                if matches!(fresh_config.provider_type.as_str(), "argocd" | "fluxcd") {
                    return Vec::new();
                }

                items
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .map(|item| {
                        if fresh_config.provider_type == "tekton" && item.contains("__") {
                            item.replace("__", "/")
                        } else if fresh_config.provider_type == "jenkins" && !item.contains('/') {
                            format!("(root)/{}", item)
                        } else {
                            item
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        const FETCH_STATUS_SUCCESS: &str = "success";
        const FETCH_STATUS_ERROR: &str = "error";
        const FETCH_STATUS_NEVER: &str = "never";

        let (fetch_status_str, last_fetch_error, last_fetch_at) = self
            .repository
            .get_provider_fetch_status(id)
            .await
            .unwrap_or_else(|_| (FETCH_STATUS_NEVER.to_string(), None, None));

        let fetch_status_enum = match fetch_status_str.as_str() {
            FETCH_STATUS_SUCCESS => FetchStatus::Success,
            FETCH_STATUS_ERROR => FetchStatus::Error,
            _ => FetchStatus::Never,
        };

        let last_fetch_at_parsed = last_fetch_at.and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        });

        let provider_summary = ProviderSummary {
            id,
            name: fresh_config.display_name().to_string(),
            provider_type: fresh_config.provider_type,
            icon,
            pipeline_count,
            last_updated,
            refresh_interval: fresh_config.refresh_interval,
            configured_repositories,
            last_fetch_status: fetch_status_enum,
            last_fetch_error,
            last_fetch_at: last_fetch_at_parsed,
            paused_until: None,
            version: fresh_config.version.unwrap_or(1),
        };

        let pipelines = cached_pipelines;

        self.repository.remove_provider(id).await?;
        self.quota_pauses.remove(&id);

        let providers = Arc::clone(&self.providers);
        tokio::spawn(async move {
            let mut map = providers.write().await;
            map.remove(&id);
            tracing::debug!(provider_id = id, "Provider cleanup completed");
        });

        let mut fetches = self.parameter_fetches.lock().await;
        for pipeline in pipelines {
            fetches.remove(&pipeline.id);
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        self.event_bus
            .emit(CoreEvent::ProviderRemoved {
                provider: provider_summary,
                timestamp,
            })
            .await;

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

    pub fn paused_until(&self, provider_id: i64) -> Option<DateTime<Utc>> {
        self.quota_pauses
            .get(&provider_id)
            .map(|until| *until)
            .filter(|until| *until > Utc::now())
    }

    pub async fn check_quota(&self, provider_id: i64) -> Option<DateTime<Utc>> {
        if let Some(until) = self.paused_until(provider_id) {
            return Some(until);
        }

        let provider = self.get_provider(provider_id).await.ok()?;
        let rate_limit = match timeout(Duration::from_secs(10), provider.fetch_rate_limit()).await {
            Ok(Ok(rate_limit)) => rate_limit,
            Ok(Err(e)) => {
                tracing::debug!(provider_id, error = %e, "Rate limit check failed");
                None
            }
            Err(_) => {
                tracing::debug!(provider_id, "Rate limit check timed out");
                None
            }
        };
        let paused_until = rate_limit.and_then(|rate_limit| quota_pause(&rate_limit, Utc::now()));
        let changed = match paused_until {
            Some(until) => {
                tracing::info!(provider_id, %until, "API quota almost used up, pausing provider");
                self.quota_pauses.insert(provider_id, until) != Some(until)
            }
            None => self.quota_pauses.remove(&provider_id).is_some(),
        };
        if changed {
            self.event_bus.emit(CoreEvent::ProvidersChanged).await;
        }

        paused_until
    }

    pub async fn get_provider_instance(&self, id: i64) -> DomainResult<Arc<dyn Provider>> {
        self.get_provider(id).await
    }

    pub async fn load_all_providers(&self) -> DomainResult<()> {
        let configs = self.repository.list_providers().await?;
        let mut old_providers_to_cleanup = Vec::new();

        for config in configs {
            if let Some(id) = config.id {
                if config.token.is_empty() {
                    tracing::debug!(
                        provider_id = id,
                        provider_type = %config.provider_type,
                        "Skipping provider load - token not available yet (vault initializing in background)"
                    );
                    continue;
                }

                match self.create_provider(&config) {
                    Ok(provider) => {
                        let mut providers = self.providers.write().await;
                        if let Some(old) = providers.insert(id, provider) {
                            old_providers_to_cleanup.push((id, old));
                        }
                    }
                    Err(e) => {
                        tracing::warn!(provider_id = id, error = %e, "Failed to load provider");
                    }
                }
            }
        }

        if !old_providers_to_cleanup.is_empty() {
            tokio::spawn(async move {
                for (id, provider) in old_providers_to_cleanup {
                    drop(provider);
                    tracing::debug!(
                        provider_id = id,
                        "Old provider cleanup completed after reload"
                    );
                }
            });
        }

        Ok(())
    }

    fn create_provider(&self, config: &ProviderConfig) -> DomainResult<Arc<dyn Provider>> {
        if !self.plugin_registry.is_registered(&config.provider_type) {
            return Err(DomainError::InvalidProviderType(format!(
                "Plugin not found for provider type: {}",
                config.provider_type
            )));
        }

        let mut plugin = self.create_uninitialized_plugin(&config.provider_type)?;

        let provider_id = config.id.unwrap_or(0);
        let mut plugin_config = config.config.clone();

        let resolved_token = if config.token.is_empty() {
            String::new()
        } else {
            match TokenReference::parse(&config.token) {
                Ok(TokenReference::EnvVar(var_name)) => {
                    std::env::var(&var_name).unwrap_or_else(|_| {
                        tracing::warn!(
                            provider_id = provider_id,
                            var_name = %var_name,
                            "Environment variable not found, using empty token"
                        );
                        String::new()
                    })
                }
                Ok(TokenReference::None) => String::new(),
                Ok(_) => config.token.clone(),
                Err(_) => config.token.clone(),
            }
        };
        plugin_config.insert("token".to_string(), resolved_token);

        let http_client = if let Some(base_url) = plugin_config.get("base_url") {
            self.http_client_manager.client_for_url(base_url)?
        } else {
            self.http_client_manager.default_client()
        };

        plugin
            .initialize(provider_id, plugin_config, Some(http_client))
            .map_err(|e| DomainError::InvalidConfig(format!("Failed to initialize plugin: {e}")))?;

        let adapter = PluginAdapter::new(plugin, config.provider_type.clone(), provider_id);
        Ok(Arc::new(adapter))
    }

    pub async fn get_workflow_parameters(
        &self, provider_id: i64, workflow_id: &str,
    ) -> DomainResult<Vec<pipedash_plugin_api::WorkflowParameter>> {
        if let Ok(Some(cached)) = self
            .repository
            .get_cached_workflow_parameters(workflow_id)
            .await
        {
            return Ok(cached);
        }

        let fetch_lock = {
            let mut fetches = self.parameter_fetches.lock().await;
            fetches
                .entry(workflow_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };

        let _guard = fetch_lock.lock().await;

        if let Ok(Some(cached)) = self
            .repository
            .get_cached_workflow_parameters(workflow_id)
            .await
        {
            return Ok(cached);
        }

        let providers = self.providers.read().await;
        let provider = providers.get(&provider_id).ok_or_else(|| {
            DomainError::ProviderNotFound(format!("Provider {provider_id} not found"))
        })?;

        let parameters = provider
            .get_workflow_parameters(workflow_id)
            .await
            .map_err(|e| {
                DomainError::InvalidConfig(format!("Failed to get workflow parameters: {e}"))
            })?;

        let _ = self
            .repository
            .cache_workflow_parameters(workflow_id, &parameters)
            .await;

        {
            let mut fetches = self.parameter_fetches.lock().await;
            fetches.remove(workflow_id);
        }

        Ok(parameters)
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
        let mut plugin = self.create_uninitialized_plugin(&config.provider_type)?;

        let mut plugin_config = config.config.clone();
        plugin_config.insert("token".to_string(), config.token.clone());

        let http_client = if let Some(base_url) = plugin_config.get("base_url") {
            self.http_client_manager.client_for_url(base_url)?
        } else {
            self.http_client_manager.default_client()
        };

        plugin
            .initialize(provider_id, plugin_config, Some(http_client))
            .map_err(|e| DomainError::InvalidConfig(format!("Failed to initialize plugin: {e}")))?;

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

fn quota_pause(rate_limit: &RateLimit, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let low = rate_limit.remaining * 100 < rate_limit.limit * QUOTA_PAUSE_PERCENT;

    (low && rate_limit.reset_at > now).then_some(rate_limit.reset_at)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::super::pipeline_service::PipelineService;
    use super::*;
    use crate::event::NoOpEventBus;
    use crate::infrastructure::config::ConfigLoader;
    use crate::infrastructure::database::{
        init_database,
        DatabasePool,
    };
    use crate::infrastructure::{
        HttpClientManager,
        MemoryTokenStore,
        SqliteConfigBackend,
    };
    use crate::test_env::EnvGuard;

    const GITHUB_SECRET: &str = "ghp_exampleSECRET0123456789";
    const GITLAB_SECRET: &str = "glpat-SECRET-VALUE";
    const JENKINS_DEFAULT_SECRET: &str = "hunter2-default";

    async fn temp_repository(temp_dir: &TempDir, name: &str) -> Arc<Repository> {
        let pool = init_database(temp_dir.path().join(name)).await.unwrap();

        Arc::new(Repository::new(
            Arc::new(SqliteConfigBackend::new(pool.clone())),
            DatabasePool::Sqlite(pool),
            Arc::new(MemoryTokenStore::new()),
        ))
    }

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

    #[tokio::test]
    async fn test_export_providers_toml_round_trips_without_tokens() {
        let temp_dir = TempDir::new().unwrap();
        let repository = temp_repository(&temp_dir, "source.db").await;
        let service = ProviderService::new(
            repository.clone(),
            Arc::new(HttpClientManager::new().unwrap()),
            Arc::new(NoOpEventBus),
        );

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

        let target = temp_repository(&temp_dir, "target.db").await;
        ProviderSyncService::new()
            .sync_toml_to_db(
                parsed.providers,
                target.config_backend().as_ref(),
                &MemoryTokenStore::new(),
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

    async fn stub_github(remaining: u64, reset: i64) -> (String, Arc<Mutex<Vec<String>>>) {
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
        let temp_dir = TempDir::new().unwrap();
        let repository = temp_repository(&temp_dir, "quota.db").await;
        let reset = Utc::now().timestamp() + 1200;
        let (base_url, requests) = stub_github(0, reset).await;
        let id = repository
            .add_provider(&provider(
                "github",
                "github",
                "ghp_stub",
                30,
                &[("base_url", &base_url), ("selected_items", "acme/app")],
            ))
            .await
            .unwrap();
        let provider_service = Arc::new(ProviderService::new(
            repository.clone(),
            Arc::new(HttpClientManager::new().unwrap()),
            Arc::new(NoOpEventBus),
        ));
        let pipeline_service = PipelineService::new(
            repository,
            provider_service.clone(),
            None,
            Arc::new(NoOpEventBus),
        );

        pipeline_service.fetch_providers(vec![id]).await.unwrap();
        pipeline_service.fetch_pipelines(Some(id)).await.unwrap();
        let summaries = provider_service.list_providers().await.unwrap();

        assert_eq!(*requests.lock().await, ["/api/v3/rate_limit"]);
        assert_eq!(
            summaries[0].paused_until,
            DateTime::from_timestamp(reset, 0)
        );
        assert!(matches!(summaries[0].last_fetch_status, FetchStatus::Never));
    }

    #[tokio::test]
    async fn refresh_fetches_a_github_provider_with_quota_left() {
        let temp_dir = TempDir::new().unwrap();
        let repository = temp_repository(&temp_dir, "quota.db").await;
        let (base_url, requests) = stub_github(4000, Utc::now().timestamp() + 1200).await;
        let id = repository
            .add_provider(&provider(
                "github",
                "github",
                "ghp_stub",
                30,
                &[("base_url", &base_url), ("selected_items", "acme/app")],
            ))
            .await
            .unwrap();
        let provider_service = Arc::new(ProviderService::new(
            repository.clone(),
            Arc::new(HttpClientManager::new().unwrap()),
            Arc::new(NoOpEventBus),
        ));
        let pipeline_service = PipelineService::new(
            repository,
            provider_service.clone(),
            None,
            Arc::new(NoOpEventBus),
        );

        pipeline_service.fetch_providers(vec![id]).await.unwrap();
        let summaries = provider_service.list_providers().await.unwrap();
        let requests = requests.lock().await;

        assert_eq!(requests[0], "/api/v3/rate_limit");
        assert!(requests
            .iter()
            .any(|path| path.starts_with("/api/v3/repos/acme/app")));
        assert_eq!(summaries[0].paused_until, None);
    }
}
