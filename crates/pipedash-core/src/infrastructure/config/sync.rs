use std::collections::{
    HashMap,
    HashSet,
};

use indexmap::IndexMap;

use super::schema::ProviderFileConfig;
use super::state::ConfigState;
use super::token_ref::TokenReference;
use crate::domain::{
    DomainResult,
    ProviderConfig,
};
use crate::infrastructure::{
    ConfigBackend,
    TokenStore,
};

const EXPORT_HEADER: &str = "# Pipedash providers export.\n# Tokens are not included. Set each token variable in the environment or enter the token in Settings > Providers.\n\n";

const EXPORT_TOKEN_PREFIX: &str = "PIPEDASH_TOKEN_";

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
            let (provider_id, created) = match db_providers.iter().find(|p| p.name == id) {
                None => {
                    let provider_id = self
                        .create_from_toml(&id, &toml_provider, config_backend)
                        .await?;
                    result.added.push(id.clone());
                    (provider_id, true)
                }
                Some(db_provider) => {
                    let Some(db_id) = db_provider.id else {
                        continue;
                    };
                    if !self
                        .update_from_toml(&id, &toml_provider, db_provider, config_backend)
                        .await?
                    {
                        continue;
                    }
                    result.updated.push(id.clone());
                    (db_id, false)
                }
            };

            if let Err(e) =
                Self::store_resolved_token(provider_id, &toml_provider.token, created, token_store)
                    .await
            {
                tracing::warn!(
                    provider = %id,
                    error = %e,
                    "Failed to store resolved token - provider may not work until token is re-added"
                );
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

    pub(crate) async fn create_from_toml(
        &self, key: &str, toml_provider: &ProviderFileConfig, config_backend: &dyn ConfigBackend,
    ) -> DomainResult<i64> {
        let provider_config =
            Self::provider_config(key, toml_provider, toml_provider.token.clone());
        let id = config_backend.create_provider(&provider_config).await?;

        tracing::info!(provider = %key, "Synced provider from TOML to database (new)");
        Ok(id)
    }

    pub(crate) async fn update_from_toml(
        &self, key: &str, toml_provider: &ProviderFileConfig, db_provider: &ProviderConfig,
        config_backend: &dyn ConfigBackend,
    ) -> DomainResult<bool> {
        let Some(db_id) = db_provider.id else {
            return Ok(false);
        };
        if !self.needs_update(toml_provider, db_provider) {
            return Ok(false);
        }

        let provider_config = ProviderConfig {
            id: db_provider.id,
            version: db_provider.version,
            ..Self::provider_config(key, toml_provider, toml_provider.token.clone())
        };
        config_backend
            .update_provider(db_id, &provider_config)
            .await?;

        tracing::info!(provider = %key, "Synced provider from TOML to database (updated)");
        Ok(true)
    }

    pub(crate) async fn store_resolved_token(
        id: i64, token_ref: &str, created: bool, token_store: &dyn TokenStore,
    ) -> DomainResult<()> {
        match Self::resolve_token_ref(token_ref) {
            Some(resolved_token) => token_store.store_token(id, &resolved_token).await,
            None if created => token_store.store_token(id, "").await,
            None => Ok(()),
        }
    }

    pub(crate) fn provider_config(
        key: &str, file: &ProviderFileConfig, token: String,
    ) -> ProviderConfig {
        let mut config = file.config.clone();
        if let Some(display_name) = &file.name {
            config.insert("display_name".to_string(), display_name.clone());
        }

        ProviderConfig {
            id: None,
            name: key.to_string(),
            provider_type: file.provider_type.clone(),
            token,
            config,
            refresh_interval: file.refresh_interval as i64,
            version: None,
        }
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

impl ProviderSyncService {
    pub fn export_toml(db_providers: &[ProviderConfig]) -> DomainResult<String> {
        let mut sorted: Vec<&ProviderConfig> = db_providers.iter().collect();
        sorted.sort_by_key(|p| p.id);

        let mut used_vars: HashSet<String> = sorted
            .iter()
            .filter_map(|db| Self::preserved_token_var(&db.token))
            .map(str::to_string)
            .collect();
        let mut providers = IndexMap::new();

        for db in sorted {
            let mut config: HashMap<String, String> = db.config.clone();
            let name = config.remove("display_name");

            let token = match Self::preserved_token_var(&db.token) {
                Some(var_name) => format!("${{{var_name}:-}}"),
                None => Self::generated_token_ref(&db.name, &mut used_vars),
            };

            providers.insert(
                db.name.clone(),
                ProviderFileConfig {
                    name,
                    provider_type: db.provider_type.clone(),
                    token,
                    refresh_interval: u32::try_from(db.refresh_interval)
                        .unwrap_or(super::schema::DEFAULT_REFRESH_INTERVAL_SECS),
                    config,
                },
            );
        }

        let mut doc = toml_edit::DocumentMut::new();
        ConfigState::write_providers(&mut doc, &providers);

        Ok(format!("{EXPORT_HEADER}{doc}"))
    }

    fn preserved_token_var(stored: &str) -> Option<&str> {
        let inner = stored.trim().strip_prefix("${")?.strip_suffix('}')?;
        let var_name = inner.split(":-").next().unwrap_or(inner);
        let mut chars = var_name.chars();
        let first = chars.next()?;

        if !(first.is_ascii_alphabetic() || first == '_')
            || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        {
            return None;
        }

        Some(var_name)
    }

    fn generated_token_ref(key: &str, used_vars: &mut HashSet<String>) -> String {
        let mut suffix = String::new();
        for c in key.chars() {
            if c.is_ascii_alphanumeric() {
                suffix.push(c.to_ascii_uppercase());
            } else if !suffix.ends_with('_') {
                suffix.push('_');
            }
        }
        let suffix = suffix.trim_matches('_');
        let base = format!(
            "{EXPORT_TOKEN_PREFIX}{}",
            if suffix.is_empty() {
                "PROVIDER"
            } else {
                suffix
            }
        );

        let mut var_name = base.clone();
        let mut n = 2;
        while !used_vars.insert(var_name.clone()) {
            var_name = format!("{base}_{n}");
            n += 1;
        }

        format!("${{{var_name}:-}}")
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

    #[tokio::test]
    async fn test_sync_toml_to_db_stores_an_empty_token_when_none_resolves() {
        let (backend, token_store, _temp) = create_test_backend().await;

        let mut toml_providers = IndexMap::new();
        toml_providers.insert(
            "ci-jenkins".to_string(),
            ProviderFileConfig {
                name: Some("CI Jenkins".to_string()),
                provider_type: "jenkins".to_string(),
                token: String::new(),
                refresh_interval: 60,
                config: Default::default(),
            },
        );

        ProviderSyncService::new()
            .sync_toml_to_db(toml_providers, &backend, token_store.as_ref(), true)
            .await
            .unwrap();

        let id = backend.list_providers().await.unwrap()[0].id.unwrap();
        assert_eq!(token_store.get_token(id).await.unwrap(), "");
    }

    #[test]
    fn test_export_toml_generated_var_never_reuses_preserved_var() {
        let provider = |id: i64, name: &str, token: &str| ProviderConfig {
            id: Some(id),
            name: name.to_string(),
            provider_type: "github".to_string(),
            token: token.to_string(),
            config: HashMap::new(),
            refresh_interval: 30,
            version: None,
        };
        let preserved = provider(1, "other", "${PIPEDASH_TOKEN_PROD}");
        let literal = provider(2, "prod", "ghp_literal_secret");

        for providers in [
            [preserved.clone(), literal.clone()],
            [
                literal.clone(),
                ProviderConfig {
                    id: Some(3),
                    ..preserved
                },
            ],
        ] {
            let exported = ProviderSyncService::export_toml(&providers).unwrap();

            assert!(!exported.contains("ghp_literal_secret"));
            assert_eq!(exported.matches("${PIPEDASH_TOKEN_PROD:-}").count(), 1);
            assert!(exported.contains("${PIPEDASH_TOKEN_PROD_2:-}"));
        }
    }

    #[test]
    fn test_export_toml_is_providers_only() {
        let exported = ProviderSyncService::export_toml(&[]).unwrap();

        assert!(!exported.contains("[general]"));
        assert!(exported.starts_with("# Pipedash providers export."));
    }
}
