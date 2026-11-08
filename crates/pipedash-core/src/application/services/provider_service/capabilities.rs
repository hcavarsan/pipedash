use std::collections::HashMap;

use pipedash_plugin_api::schema::TableSchema;
use pipedash_plugin_api::{
    Plugin,
    PluginMetadata,
};

use super::dto::PermissionCheckResult;
use super::ProviderService;
use crate::domain::{
    DomainError,
    DomainResult,
};

impl ProviderService {
    pub async fn check_permissions(
        &self, provider_type: &str, config: HashMap<String, String>,
    ) -> DomainResult<PermissionCheckResult> {
        let plugin = self.init_plugin(provider_type, config, 0).await?;
        let permission_status = plugin.check_permissions().await.ok();
        let features = permission_status
            .as_ref()
            .map(|status| plugin.get_feature_availability(status))
            .unwrap_or_default();

        Ok(PermissionCheckResult {
            permission_status,
            features,
        })
    }

    pub async fn get_provider_table_schema(&self, provider_id: i64) -> DomainResult<TableSchema> {
        Ok(self.plugin_metadata_for(provider_id).await?.table_schema)
    }

    pub(super) async fn plugin_metadata_for(
        &self, provider_id: i64,
    ) -> DomainResult<PluginMetadata> {
        let config = self.get_provider_config(provider_id).await?;

        Ok(self
            .registered_plugin(&config.provider_type)?
            .metadata()
            .clone())
    }

    fn registered_plugin(&self, provider_type: &str) -> DomainResult<&dyn Plugin> {
        self.plugin_registry.get(provider_type).ok_or_else(|| {
            DomainError::NotFound(format!(
                "Plugin not found for provider type: {provider_type}"
            ))
        })
    }
}

#[cfg(test)]
mod tests;
