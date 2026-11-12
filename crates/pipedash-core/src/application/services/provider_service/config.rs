use std::collections::HashSet;

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
use crate::infrastructure::TokenStore;

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

    pub async fn apply_config_file(&self, config_state: &ConfigState) -> DomainResult<SyncResult> {
        let _apply = self.config_apply.lock().await;
        let loaded = config_state.load_file().await?;

        self.apply_loaded_config(config_state, loaded).await
    }

    async fn apply_loaded_config(
        &self, config_state: &ConfigState, loaded: PipedashConfig,
    ) -> DomainResult<SyncResult> {
        let PipedashConfig {
            providers: file,
            views,
            general,
            ..
        } = loaded;
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
            let mut failure = None;
            for (key, provider) in &changed {
                let id = existing.iter().find(|p| p.name == *key).and_then(|p| p.id);
                if let Err(e) = self
                    .apply_config_provider(key, provider, id, token_store.as_ref())
                    .await
                {
                    failure = Some(e);
                    break;
                }
                config_state.remember_provider(key, provider).await;
                match id {
                    Some(_) => result.updated.push(key.clone()),
                    None => result.added.push(key.clone()),
                }
            }

            if !result.added.is_empty() || !result.updated.is_empty() {
                tracing::info!(
                    added = result.added.len(),
                    updated = result.updated.len(),
                    "Applied provider changes from config file"
                );
                self.event_bus.emit(CoreEvent::ProvidersChanged).await;
            }
            if let Some(e) = failure {
                return Err(e);
            }
        }
        if current.views != views {
            config_state.remember_views(views).await;
            self.event_bus.emit(CoreEvent::PipelineViewsChanged).await;
        }
        config_state.remember_providers(file).await;
        if current.general != general {
            let proxy_changed = self.http_client_manager.set_proxy(&general.proxy_url)?;
            config_state.remember_general(general).await;
            if proxy_changed {
                self.rebuild_loaded_providers().await;
            }
        }

        Ok(result)
    }

    pub async fn update_general_settings(
        &self, config_state: &ConfigState, update: GeneralSettingsUpdate,
    ) -> DomainResult<()> {
        let _apply = self.config_apply.lock().await;
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
            &self.plugin_registry,
        )
        .await?;

        let config = self.repository.get_provider(id).await?;
        let instance = (self.provider_factory)(&config)?;
        self.reset_fetch_status(id).await;
        let lease = self.replace(id, instance, config.refresh_interval).await;
        self.purge_run_logs(id).await;
        self.health.send(HealthInput::ProvidersChanged {
            changed: vec![self.health_input(&config, &lease)],
        });
        drop(guard);

        self.queue_fetch(&lease);

        Ok(())
    }
}
