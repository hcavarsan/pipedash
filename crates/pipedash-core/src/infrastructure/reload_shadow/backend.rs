use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use super::ReloadShadow;
use crate::domain::{
    DomainResult,
    ProviderConfig,
};
use crate::infrastructure::config_backend::{
    ConfigExport,
    StoredPermissions,
};
use crate::infrastructure::ConfigBackend;

pub(crate) struct ShadowedBackend {
    inner: Arc<dyn ConfigBackend>,
    shadow: Arc<ReloadShadow>,
}

impl ShadowedBackend {
    pub(crate) fn new(inner: Arc<dyn ConfigBackend>, shadow: Arc<ReloadShadow>) -> Self {
        Self { inner, shadow }
    }
}

#[async_trait]
impl ConfigBackend for ShadowedBackend {
    async fn list_providers(&self) -> DomainResult<Vec<ProviderConfig>> {
        Ok(self.shadow.rows(self.inner.list_providers().await?))
    }

    async fn get_provider(&self, id: i64) -> DomainResult<Option<ProviderConfig>> {
        Ok(self.shadow.row(self.inner.get_provider(id).await?))
    }

    async fn create_provider(&self, config: &ProviderConfig) -> DomainResult<i64> {
        self.inner.create_provider(config).await
    }

    async fn update_provider(&self, id: i64, config: &ProviderConfig) -> DomainResult<()> {
        self.inner.update_provider(id, config).await
    }

    async fn delete_provider(&self, id: i64) -> DomainResult<()> {
        self.inner.delete_provider(id).await
    }

    async fn mark_config_file_providers(&self, names: &[String]) -> DomainResult<()> {
        self.inner.mark_config_file_providers(names).await
    }

    async fn list_config_file_providers(&self) -> DomainResult<Vec<(i64, String)>> {
        self.inner.list_config_file_providers().await
    }

    async fn record_config_file_hash(&self, id: i64, hash: &str) -> DomainResult<()> {
        self.inner.record_config_file_hash(id, hash).await
    }

    async fn config_file_hash(&self, id: i64) -> DomainResult<Option<String>> {
        self.inner.config_file_hash(id).await
    }

    async fn get_table_preferences(
        &self, provider_id: i64, table_id: &str,
    ) -> DomainResult<Option<String>> {
        self.inner
            .get_table_preferences(provider_id, table_id)
            .await
    }

    async fn set_table_preferences(
        &self, provider_id: i64, table_id: &str, preferences_json: &str,
    ) -> DomainResult<()> {
        self.inner
            .set_table_preferences(provider_id, table_id, preferences_json)
            .await
    }

    async fn store_permissions(
        &self, provider_id: i64, permissions: &StoredPermissions,
    ) -> DomainResult<()> {
        self.inner.store_permissions(provider_id, permissions).await
    }

    async fn export_all(&self) -> DomainResult<ConfigExport> {
        let mut export = self.inner.export_all().await?;
        export.providers = self.shadow.rows(export.providers);

        Ok(export)
    }

    async fn import_all(&self, data: &ConfigExport) -> DomainResult<HashMap<i64, i64>> {
        self.inner.import_all(data).await
    }
}
