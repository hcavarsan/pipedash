use pipedash_plugin_api::schema::TableDefinition;

use super::dto::DefaultPreferences;
use super::ProviderService;
use crate::domain::{
    DomainError,
    DomainResult,
};

fn default_preferences(table: &TableDefinition) -> DefaultPreferences {
    DefaultPreferences {
        column_order: table.columns.iter().map(|col| col.id.clone()).collect(),
        column_visibility: table
            .columns
            .iter()
            .map(|col| (col.id.clone(), col.default_visible))
            .collect(),
    }
}

impl ProviderService {
    pub async fn table_preferences(
        &self, provider_id: i64, table_id: &str,
    ) -> DomainResult<Option<String>> {
        self.repository
            .get_table_preferences(provider_id, table_id)
            .await
    }

    pub async fn save_table_preferences(
        &self, provider_id: i64, table_id: &str, preferences_json: &str,
    ) -> DomainResult<()> {
        self.repository
            .upsert_table_preferences(provider_id, table_id, preferences_json)
            .await
    }

    pub async fn default_table_preferences(
        &self, provider_id: i64, table_id: &str,
    ) -> DomainResult<DefaultPreferences> {
        let metadata = self.plugin_metadata_for(provider_id).await?;
        let table = metadata.table_schema.get_table(table_id).ok_or_else(|| {
            DomainError::NotFound(format!("Table '{table_id}' not found in schema"))
        })?;

        Ok(default_preferences(table))
    }
}

#[cfg(test)]
mod tests;
