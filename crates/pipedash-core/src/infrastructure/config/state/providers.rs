use std::sync::Arc;

use indexmap::IndexMap;

use super::persistence::{
    parse_document,
    read_optional,
    write_file,
};
use super::ConfigState;
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::config::loader::ConfigLoader;
use crate::infrastructure::config::schema::{
    PipedashConfig,
    ProviderFileConfig,
    ViewFileConfig,
};
use crate::infrastructure::config::sync::SyncResult;
use crate::infrastructure::config::validation::ConfigValidator;
use crate::infrastructure::ConfigBackend;

impl ConfigState {
    pub async fn remove_provider(&self, id: &str) -> DomainResult<()> {
        if let Some(content) = read_optional(&self.config_path).await? {
            let mut doc = parse_document(&content)?;
            if doc
                .get_mut("providers")
                .and_then(toml_edit::Item::as_table_like_mut)
                .and_then(|providers| providers.remove(id))
                .is_some()
            {
                write_file(&self.config_path, doc.to_string()).await?;
            }
        }

        let mut current = self.config.write().await;
        if current.providers.contains_key(id) {
            let mut config = (**current).clone();
            config.providers.shift_remove(id);
            *current = Arc::new(config);
        }
        Ok(())
    }

    pub async fn sync_providers_from_toml(
        &self, config_backend: &dyn ConfigBackend, delete_orphans: bool,
    ) -> DomainResult<SyncResult> {
        let providers = self.config.read().await.providers.clone();
        self.sync_service
            .sync_toml_to_db(
                providers,
                config_backend,
                self.token_store.as_ref(),
                delete_orphans,
                &self.plugin_registry,
            )
            .await
    }

    pub async fn load_file(&self) -> DomainResult<PipedashConfig> {
        let path = self.config_path.clone();
        let file = tokio::task::spawn_blocking(move || ConfigLoader::load(&path))
            .await
            .map_err(|e| DomainError::InternalError(format!("Config load task failed: {e}")))?
            .map_err(|e| DomainError::InvalidConfig(format!("Failed to reload config: {}", e)))?;

        let validation = ConfigValidator::validate_with_plugins(&file, &self.plugin_registry);
        if !validation.errors.is_empty() {
            return Err(DomainError::InvalidConfig(format!(
                "Config validation failed: {:?}",
                validation.errors
            )));
        }

        Ok(file)
    }

    pub async fn load_providers(&self) -> DomainResult<IndexMap<String, ProviderFileConfig>> {
        self.load_file().await.map(|file| file.providers)
    }

    pub async fn remember_provider(&self, id: &str, provider: &ProviderFileConfig) {
        let mut config = self.config.write().await;
        let mut next = (**config).clone();
        next.providers.insert(id.to_string(), provider.clone());
        *config = Arc::new(next);
    }

    pub async fn remember_providers(&self, providers: IndexMap<String, ProviderFileConfig>) {
        let mut config = self.config.write().await;
        let mut next = (**config).clone();
        next.providers = providers;
        *config = Arc::new(next);
    }

    pub async fn remember_views(&self, views: IndexMap<String, ViewFileConfig>) {
        let mut config = self.config.write().await;
        let mut next = (**config).clone();
        next.views = views;
        *config = Arc::new(next);
    }
}
