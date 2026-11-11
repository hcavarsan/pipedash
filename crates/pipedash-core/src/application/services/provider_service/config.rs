use std::collections::HashSet;
use std::sync::Arc;

mod staged;

use super::ProviderService;
use crate::application::services::health_service::HealthInput;
use crate::domain::{
    DomainError,
    DomainResult,
    ProviderImportFailure,
    ProviderImportResult,
};
use crate::event::CoreEvent;
use crate::infrastructure::config::sync::{
    ProviderSyncService,
    SyncResult,
};
use crate::infrastructure::config::{
    ConfigLoader,
    ConfigState,
    GeneralSettingsUpdate,
    PipedashConfig,
    ProviderFileConfig,
};
use crate::infrastructure::reload_shadow::ReloadShadow;

impl ProviderService {
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
        let mut remaining = file.providers.iter();

        for (key, provider) in remaining.by_ref() {
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
                        result.failed.push(ProviderImportFailure {
                            name,
                            error: e.to_string(),
                        });
                        break;
                    }
                    result.skipped.push(name);
                }
            }
        }
        result.not_attempted = remaining
            .map(|(key, provider)| provider.display_name(key).to_string())
            .collect();

        if !result.added.is_empty() {
            self.event_bus.emit(CoreEvent::ProvidersChanged).await;
        }

        Ok(result)
    }

    pub async fn apply_config_file(
        self: &Arc<Self>, config_state: &Arc<ConfigState>,
    ) -> DomainResult<SyncResult> {
        let loaded = match config_state.load_file().await {
            Ok(loaded) => loaded,
            Err(e) => {
                self.config_failed(e.to_string()).await;
                return Err(e);
            }
        };

        self.apply_loaded_config(config_state, loaded).await
    }

    pub(crate) async fn apply_loaded_config(
        self: &Arc<Self>, config_state: &Arc<ConfigState>, loaded: PipedashConfig,
    ) -> DomainResult<SyncResult> {
        let service = Arc::clone(self);
        let config_state = Arc::clone(config_state);
        let done = self.own_reload(ReloadShadow::reveal(async move {
            let result = service.apply_loaded_inner(&config_state, loaded).await;
            if let Err(e) = &result {
                service.config_failed(e.to_string()).await;
            }
            result
        }))?;

        done.await.map_err(|_| {
            DomainError::InternalError("The config reload ended without a result".into())
        })?
    }

    async fn apply_loaded_inner(
        &self, config_state: &ConfigState, loaded: PipedashConfig,
    ) -> DomainResult<SyncResult> {
        let _apply = self.config_apply.lock().await;
        let current = config_state.get().await;
        if let Some(applications) = self.applications.get() {
            if loaded.applications != current.applications
                || loaded.correlation != current.correlation
            {
                applications.check(&loaded).await?;
            }
        }
        let changed: Vec<(String, ProviderFileConfig)> = loaded
            .providers
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

        let token_store = config_state.token_store();
        let (staged, shadow) = self.stage_providers(&changed, token_store.as_ref()).await?;
        if let Some(applications) = self.applications.get() {
            if let Err(error) = applications.commit(&loaded).await {
                return Err(self.unstage(staged, error).await);
            }
        }

        let mut result = SyncResult::default();
        for one in &staged {
            if one.created {
                result.added.push(one.key.clone());
            } else {
                result.updated.push(one.key.clone());
            }
        }
        let leases = self.activate(staged).await;
        drop(shadow);
        let views_changed = current.views != loaded.views;
        if views_changed {
            config_state.remember_views(loaded.views).await;
        }
        config_state.remember_providers(loaded.providers).await;

        if !leases.is_empty() {
            tracing::info!(
                added = result.added.len(),
                updated = result.updated.len(),
                "Applied provider changes from config file"
            );
            self.event_bus.emit(CoreEvent::ProvidersChanged).await;
        }
        if views_changed {
            self.event_bus.emit(CoreEvent::PipelineViewsChanged).await;
        }
        if let Some(applications) = self.applications.get() {
            applications.announce().await;
        }
        for lease in &leases {
            self.queue_fetch(lease);
        }

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
}
