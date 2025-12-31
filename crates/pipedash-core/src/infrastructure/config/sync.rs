use std::collections::HashSet;

use indexmap::IndexMap;

use super::schema::ProviderFileConfig;
use super::token_ref::TokenReference;
use crate::domain::{
    DomainResult,
    ProviderConfig,
};
use crate::infrastructure::{
    ConfigBackend,
    TokenStore,
};

#[derive(Debug, Clone, Default)]
pub struct SyncResult {
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
}

pub struct ProviderSyncService;

impl ProviderSyncService {
    pub fn new() -> Self {
        Self
    }

    pub async fn sync_toml_to_db(
        &self, toml_providers: IndexMap<String, ProviderFileConfig>,
        config_backend: &dyn ConfigBackend, token_store: &dyn TokenStore, delete_orphans: bool,
    ) -> DomainResult<SyncResult> {
        let mut result = SyncResult::default();

        let db_providers = config_backend.list_providers().await?;

        let toml_ids: HashSet<String> = toml_providers.keys().cloned().collect();

        for (id, toml_provider) in toml_providers {
            let existing_db = db_providers.iter().find(|p| p.name == id);

            match existing_db {
                None => {
                    let mut config = toml_provider.config.clone();
                    if let Some(display_name) = &toml_provider.name {
                        config.insert("display_name".to_string(), display_name.clone());
                    }

                    let provider_config = ProviderConfig {
                        id: None,
                        name: id.clone(), // ID for database lookups
                        provider_type: toml_provider.provider_type.clone(),
                        token: toml_provider.token.clone(),
                        config,
                        refresh_interval: toml_provider.refresh_interval as i64,
                        version: None,
                    };

                    let new_provider_id = config_backend.create_provider(&provider_config).await?;

                    if let Some(resolved_token) = Self::resolve_token_ref(&toml_provider.token) {
                        if let Err(e) = token_store
                            .store_token(new_provider_id, &resolved_token)
                            .await
                        {
                            tracing::warn!(
                                provider = %id,
                                error = %e,
                                "Failed to store resolved token - provider may not work until token is re-added"
                            );
                        }
                    }

                    result.added.push(id.clone());

                    tracing::info!(
                        provider = %id,
                        "Synced provider from TOML to database (new)"
                    );
                }
                Some(db_provider) => {
                    if self.needs_update(&toml_provider, db_provider) {
                        let mut config = toml_provider.config.clone();
                        if let Some(display_name) = &toml_provider.name {
                            config.insert("display_name".to_string(), display_name.clone());
                        }

                        let provider_config = ProviderConfig {
                            id: db_provider.id,
                            name: id.clone(), // ID for database lookups
                            provider_type: toml_provider.provider_type.clone(),
                            token: toml_provider.token.clone(),
                            config,
                            refresh_interval: toml_provider.refresh_interval as i64,
                            version: db_provider.version,
                        };

                        if let Some(db_id) = db_provider.id {
                            config_backend
                                .update_provider(db_id, &provider_config)
                                .await?;

                            if let Some(resolved_token) =
                                Self::resolve_token_ref(&toml_provider.token)
                            {
                                if let Err(e) =
                                    token_store.store_token(db_id, &resolved_token).await
                                {
                                    tracing::warn!(
                                        provider = %id,
                                        error = %e,
                                        "Failed to store resolved token - provider may not work until token is re-added"
                                    );
                                }
                            }

                            result.updated.push(id.clone());

                            tracing::info!(
                                provider = %id,
                                "Synced provider from TOML to database (updated)"
                            );
                        }
                    }
                }
            }
        }

        if delete_orphans {
            for db_provider in db_providers {
                if !toml_ids.contains(&db_provider.name) {
                    if let Some(id) = db_provider.id {
                        config_backend.delete_provider(id).await?;
                        result.removed.push(db_provider.name.clone());

                        tracing::info!(
                            provider = %db_provider.name,
                            "Removed provider from database (not in TOML)"
                        );
                    }
                }
            }
        }

        Ok(result)
    }

    fn needs_update(&self, toml: &ProviderFileConfig, db: &ProviderConfig) -> bool {
        if toml.provider_type != db.provider_type
            || toml.token != db.token
            || toml.refresh_interval as i64 != db.refresh_interval
        {
            return true;
        }

        let db_display_name = db.config.get("display_name").map(|s| s.as_str());
        let toml_display_name = toml.name.as_deref();
        if db_display_name != toml_display_name {
            return true;
        }

        let db_config_without_display: std::collections::HashMap<_, _> = db
            .config
            .iter()
            .filter(|(k, _)| *k != "display_name")
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        toml.config != db_config_without_display
    }

    fn resolve_token_ref(token_ref: &str) -> Option<String> {
        if token_ref.is_empty() {
            return None;
        }

        match TokenReference::parse(token_ref) {
            Ok(TokenReference::EnvVar(var_name)) => match std::env::var(&var_name) {
                Ok(value) => {
                    tracing::debug!(var_name = %var_name, "Resolved token from environment variable");
                    Some(value)
                }
                Err(_) => {
                    tracing::warn!(
                        var_name = %var_name,
                        "Environment variable not found for token reference"
                    );
                    None
                }
            },
            Ok(TokenReference::None) => None,
            Ok(_) => {
                tracing::warn!(
                    token_ref = %token_ref,
                    "Unsupported token reference format in TOML config"
                );
                None
            }
            Err(_) => {
                tracing::debug!("Using token value directly (not a reference)");
                Some(token_ref.to_string())
            }
        }
    }
}

impl Default for ProviderSyncService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tempfile::TempDir;

    use super::*;
    use crate::infrastructure::database::SqliteConfigBackend;
    use crate::infrastructure::MemoryTokenStore;

    async fn create_test_backend() -> (SqliteConfigBackend, Arc<MemoryTokenStore>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let pool = crate::infrastructure::database::init_database(db_path)
            .await
            .unwrap();
        let backend = SqliteConfigBackend::new(pool);
        let token_store = Arc::new(MemoryTokenStore::new());
        (backend, token_store, temp_dir)
    }

    #[tokio::test]
    async fn test_sync_toml_to_db_new_provider() {
        let (backend, token_store, _temp) = create_test_backend().await;
        let sync_service = ProviderSyncService::new();

        let mut toml_providers = IndexMap::new();
        toml_providers.insert(
            "test-github".to_string(),
            ProviderFileConfig {
                name: None,
                provider_type: "github".to_string(),
                token: "${GITHUB_TOKEN}".to_string(),
                refresh_interval: 30,
                config: Default::default(),
            },
        );

        let result = sync_service
            .sync_toml_to_db(toml_providers, &backend, token_store.as_ref(), true)
            .await
            .unwrap();

        assert_eq!(result.added.len(), 1);
        assert_eq!(result.added[0], "test-github");
        assert!(result.updated.is_empty());
        assert!(result.removed.is_empty());

        let db_providers = backend.list_providers().await.unwrap();
        assert_eq!(db_providers.len(), 1);
        assert_eq!(db_providers[0].name, "test-github");
    }

    #[tokio::test]
    async fn test_sync_toml_to_db_update_provider() {
        let (backend, token_store, _temp) = create_test_backend().await;
        let sync_service = ProviderSyncService::new();

        let initial_provider = ProviderConfig {
            id: None,
            name: "test-github".to_string(),
            provider_type: "github".to_string(),
            token: "${OLD_TOKEN}".to_string(),
            config: Default::default(),
            refresh_interval: 30,
            version: None,
        };
        backend.create_provider(&initial_provider).await.unwrap();

        let mut updated_toml = IndexMap::new();
        updated_toml.insert(
            "test-github".to_string(),
            ProviderFileConfig {
                name: None,
                provider_type: "github".to_string(),
                token: "${NEW_TOKEN}".to_string(),
                refresh_interval: 60,
                config: Default::default(),
            },
        );

        let result = sync_service
            .sync_toml_to_db(updated_toml, &backend, token_store.as_ref(), true)
            .await
            .unwrap();

        assert!(result.added.is_empty());
        assert_eq!(result.updated.len(), 1);
        assert_eq!(result.updated[0], "test-github");
        assert!(result.removed.is_empty());

        let db_providers = backend.list_providers().await.unwrap();
        assert_eq!(db_providers[0].token, "${NEW_TOKEN}");
        assert_eq!(db_providers[0].refresh_interval, 60);
    }

    #[tokio::test]
    async fn test_sync_toml_to_db_remove_provider() {
        let (backend, token_store, _temp) = create_test_backend().await;
        let sync_service = ProviderSyncService::new();

        let initial_provider = ProviderConfig {
            id: None,
            name: "test-github".to_string(),
            provider_type: "github".to_string(),
            token: "${TOKEN}".to_string(),
            config: Default::default(),
            refresh_interval: 30,
            version: None,
        };
        backend.create_provider(&initial_provider).await.unwrap();

        let result = sync_service
            .sync_toml_to_db(IndexMap::new(), &backend, token_store.as_ref(), true)
            .await
            .unwrap();

        assert!(result.added.is_empty());
        assert!(result.updated.is_empty());
        assert_eq!(result.removed.len(), 1);
        assert_eq!(result.removed[0], "test-github");

        let db_providers = backend.list_providers().await.unwrap();
        assert!(db_providers.is_empty());
    }

    #[tokio::test]
    async fn test_sync_toml_to_db_keep_orphans_for_postgres() {
        let (backend, token_store, _temp) = create_test_backend().await;
        let sync_service = ProviderSyncService::new();

        let db_only_provider = ProviderConfig {
            id: None,
            name: "db-only-provider".to_string(),
            provider_type: "github".to_string(),
            token: "${DB_TOKEN}".to_string(),
            config: Default::default(),
            refresh_interval: 30,
            version: None,
        };
        backend.create_provider(&db_only_provider).await.unwrap();

        let result = sync_service
            .sync_toml_to_db(IndexMap::new(), &backend, token_store.as_ref(), false)
            .await
            .unwrap();

        assert!(result.added.is_empty());
        assert!(result.updated.is_empty());
        assert!(result.removed.is_empty());

        let db_providers = backend.list_providers().await.unwrap();
        assert_eq!(db_providers.len(), 1);
        assert_eq!(db_providers[0].name, "db-only-provider");
    }

    #[tokio::test]
    async fn test_sync_toml_to_db_postgres_mode_adds_from_toml() {
        let (backend, token_store, _temp) = create_test_backend().await;
        let sync_service = ProviderSyncService::new();

        let db_only_provider = ProviderConfig {
            id: None,
            name: "db-only-provider".to_string(),
            provider_type: "github".to_string(),
            token: "${DB_TOKEN}".to_string(),
            config: Default::default(),
            refresh_interval: 30,
            version: None,
        };
        backend.create_provider(&db_only_provider).await.unwrap();

        let mut toml_providers = IndexMap::new();
        toml_providers.insert(
            "toml-provider".to_string(),
            ProviderFileConfig {
                name: None,
                provider_type: "gitlab".to_string(),
                token: "${TOML_TOKEN}".to_string(),
                refresh_interval: 60,
                config: Default::default(),
            },
        );

        let result = sync_service
            .sync_toml_to_db(toml_providers, &backend, token_store.as_ref(), false)
            .await
            .unwrap();

        assert_eq!(result.added.len(), 1);
        assert_eq!(result.added[0], "toml-provider");
        assert!(result.updated.is_empty());
        assert!(result.removed.is_empty());

        let db_providers = backend.list_providers().await.unwrap();
        assert_eq!(db_providers.len(), 2);
    }
}
