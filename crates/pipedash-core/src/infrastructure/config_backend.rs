use std::collections::HashMap;

use async_trait::async_trait;
use serde::{
    Deserialize,
    Serialize,
};

use crate::domain::{
    DomainResult,
    ProviderConfig,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPermissions {
    pub permissions: HashMap<String, bool>,
    pub last_checked: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigExport {
    pub version: String,
    pub providers: Vec<ProviderConfig>,
    pub table_preferences: HashMap<String, String>,
    pub permissions: HashMap<i64, StoredPermissions>,
}

impl Default for ConfigExport {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            providers: Vec::new(),
            table_preferences: HashMap::new(),
            permissions: HashMap::new(),
        }
    }
}

#[async_trait]
pub trait ConfigBackend: Send + Sync {
    async fn list_providers(&self) -> DomainResult<Vec<ProviderConfig>>;

    async fn get_provider(&self, id: i64) -> DomainResult<Option<ProviderConfig>>;

    async fn create_provider(&self, config: &ProviderConfig) -> DomainResult<i64>;

    async fn update_provider(&self, id: i64, config: &ProviderConfig) -> DomainResult<()>;

    async fn delete_provider(&self, id: i64) -> DomainResult<()>;

    async fn get_table_preferences(
        &self, provider_id: i64, table_id: &str,
    ) -> DomainResult<Option<String>>;

    async fn set_table_preferences(
        &self, provider_id: i64, table_id: &str, preferences_json: &str,
    ) -> DomainResult<()>;

    async fn store_permissions(
        &self, provider_id: i64, permissions: &StoredPermissions,
    ) -> DomainResult<()>;

    async fn get_permissions(&self, provider_id: i64) -> DomainResult<Option<StoredPermissions>>;

    async fn export_all(&self) -> DomainResult<ConfigExport>;

    async fn import_all(&self, data: &ConfigExport) -> DomainResult<HashMap<i64, i64>>;
}
