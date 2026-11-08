use std::path::Path;
use std::sync::Arc;

use toml_edit::{
    value,
    DocumentMut,
    Item,
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
};

pub(super) async fn read_optional(path: &Path) -> DomainResult<Option<String>> {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(DomainError::DatabaseError(format!(
            "Failed to read config file: {e}"
        ))),
    }
}

pub(super) fn parse_document(content: &str) -> DomainResult<DocumentMut> {
    content
        .parse()
        .map_err(|e| DomainError::DatabaseError(format!("Failed to parse existing TOML: {e}")))
}

pub(super) async fn write_file(path: &Path, content: String) -> DomainResult<()> {
    tokio::fs::write(path, content)
        .await
        .map_err(|e| DomainError::DatabaseError(format!("Failed to write config file: {e}")))
}

impl ConfigState {
    pub(super) async fn persist_general(&self, config: &PipedashConfig) -> DomainResult<()> {
        self.persist_edit(config, |doc| {
            ConfigLoader::apply_general(doc, &config.general)
        })
        .await
    }

    pub(super) async fn persist_edit(
        &self, config: &PipedashConfig, edit: impl FnOnce(&mut DocumentMut),
    ) -> DomainResult<()> {
        let content = match read_optional(&self.config_path).await? {
            Some(existing) => {
                let mut doc = parse_document(&existing)?;
                edit(&mut doc);
                doc.to_string()
            }
            None => ConfigLoader::to_toml(config).map_err(|e| {
                DomainError::DatabaseError(format!("Failed to serialize config: {}", e))
            })?,
        };

        write_file(&self.config_path, content).await?;

        *self.config.write().await = Arc::new(config.clone());

        tracing::debug!("Config persisted to {:?}", self.config_path);
        Ok(())
    }

    pub(in crate::infrastructure::config) fn write_providers(
        doc: &mut DocumentMut, providers: &indexmap::IndexMap<String, ProviderFileConfig>,
    ) {
        doc.remove("providers");

        if providers.is_empty() {
            return;
        }

        let mut providers_table = toml_edit::Table::new();

        for (id, provider) in providers {
            let mut provider_table = toml_edit::Table::new();

            if let Some(name) = &provider.name {
                provider_table["name"] = value(name);
            }

            provider_table["type"] = value(&provider.provider_type);
            provider_table["token"] = value(&provider.token);
            provider_table["refresh_interval"] = value(provider.refresh_interval as i64);

            if !provider.config.is_empty() {
                let mut config_table = toml_edit::Table::new();
                let mut entries: Vec<_> = provider.config.iter().collect();
                entries.sort();
                for (k, v) in entries {
                    config_table[k] = value(v);
                }
                provider_table["config"] = Item::Table(config_table);
            }

            providers_table[id] = Item::Table(provider_table);
        }

        doc["providers"] = Item::Table(providers_table);
    }
}
