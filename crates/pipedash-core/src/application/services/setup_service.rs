use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::setup::{
    SetupApplied,
    SetupDraft,
    SetupDraftResult,
};
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::migration::VAULT_PASSWORD_ENV_VAR;
use crate::infrastructure::{
    ConfigLoader,
    MigrationOptions,
    MigrationOrchestrator,
    MigrationStats,
    PipedashConfig,
    SchemaPostgresConfig,
    StorageBackendType,
    StorageConfig,
    StorageManager,
};
use crate::CoreContext;

const LEGACY_BUNDLE_MARKER: &str = "com.henrique.pipedash";

pub struct SetupService {
    config_path: PathBuf,
    core: Option<Arc<CoreContext>>,
    local_core: bool,
}

struct Evaluation {
    current: Option<PipedashConfig>,
    config: PipedashConfig,
    result: SetupDraftResult,
    needs_vault_password: bool,
}

fn is_mobile() -> bool {
    cfg!(any(target_os = "android", target_os = "ios"))
}

fn vault_password_from_env() -> bool {
    std::env::var(VAULT_PASSWORD_ENV_VAR).is_ok()
}

fn internal(error: impl std::fmt::Display) -> DomainError {
    DomainError::InvalidConfig(format!("Background task failed: {error}"))
}

impl SetupService {
    pub fn new(config_path: PathBuf, core: Option<Arc<CoreContext>>, local_core: bool) -> Self {
        Self {
            config_path,
            core,
            local_core,
        }
    }

    pub async fn prepare(&self, draft: &SetupDraft) -> DomainResult<SetupDraftResult> {
        Ok(self.evaluate(draft).await?.result)
    }

    pub async fn apply(&self, draft: SetupDraft) -> DomainResult<SetupApplied> {
        let evaluation = self.evaluate(&draft).await?;

        if let Some(first) = evaluation.result.errors.values().next() {
            return Err(DomainError::InvalidConfig(first.clone()));
        }

        let password = (evaluation.needs_vault_password && !draft.vault_password.is_empty())
            .then(|| draft.vault_password.clone());
        if let Some(password) = &password {
            std::env::set_var(VAULT_PASSWORD_ENV_VAR, password);
            tracing::info!("Vault password set for session (from setup wizard)");
        }

        let migration_stats = match &evaluation.current {
            Some(current) if evaluation.result.needs_migration && draft.transfer_data => {
                Some(self.migrate(current, &evaluation.config, password).await?)
            }
            _ => None,
        };

        self.save(&evaluation.config).await?;

        Ok(SetupApplied {
            saved: true,
            migration_stats,
            restart_required: evaluation.current.is_some(),
        })
    }

    async fn evaluate(&self, draft: &SetupDraft) -> DomainResult<Evaluation> {
        let backend = draft.storage_backend()?;
        let current = self.load_current().await?;
        let default_dir = PipedashConfig::default_data_dir()
            .to_string_lossy()
            .into_owned();
        let needs_vault_password = !vault_password_from_env() && !is_mobile();
        let needs_migration = self.needs_migration(current.as_ref(), draft, backend, &default_dir);
        let use_default_dir =
            self.use_default_dir(current.as_ref(), draft, needs_migration, &default_dir);

        let mut config = current.clone().unwrap_or_default();
        config.storage = StorageConfig {
            data_dir: if use_default_dir {
                String::new()
            } else {
                draft.data_dir.clone()
            },
            backend,
            postgres: if backend.requires_postgres() {
                SchemaPostgresConfig {
                    connection_string: draft.postgres_url.clone(),
                }
            } else {
                SchemaPostgresConfig::default()
            },
            vault_password: None,
        };

        Ok(Evaluation {
            current,
            result: SetupDraftResult {
                config: config.clone(),
                errors: draft.validate(needs_vault_password),
                password_warnings: draft.password_warnings(),
                needs_migration,
            },
            config,
            needs_vault_password,
        })
    }

    async fn load_current(&self) -> DomainResult<Option<PipedashConfig>> {
        let path = self.config_path.clone();
        tokio::task::spawn_blocking(move || {
            if !path.exists() {
                return None;
            }
            ConfigLoader::load(&path)
                .map_err(|e| tracing::warn!("Existing config unreadable during setup: {e}"))
                .ok()
        })
        .await
        .map_err(internal)
    }

    fn needs_migration(
        &self, current: Option<&PipedashConfig>, draft: &SetupDraft, backend: StorageBackendType,
        default_dir: &str,
    ) -> bool {
        let Some(current) = current else {
            return false;
        };
        let current_dir = current_data_dir(current, default_dir);
        let from_keyring = self.local_core
            && !is_mobile()
            && current.storage.backend.is_sqlite()
            && !vault_password_from_env();

        current.storage.backend != backend || from_keyring || current_dir != draft.data_dir
    }

    fn use_default_dir(
        &self, current: Option<&PipedashConfig>, draft: &SetupDraft, needs_migration: bool,
        default_dir: &str,
    ) -> bool {
        let current_dir = current.map_or(default_dir, |c| current_data_dir(c, default_dir));

        draft.data_dir == default_dir
            || (self.local_core
                && needs_migration
                && current_dir.contains(LEGACY_BUNDLE_MARKER)
                && draft.data_dir == current_dir)
    }

    async fn migrate(
        &self, current: &PipedashConfig, target: &PipedashConfig, password: Option<String>,
    ) -> DomainResult<MigrationStats> {
        let manager = match &self.core {
            Some(core) => {
                StorageManager::with_token_store(
                    current.clone(),
                    core.token_store.clone(),
                    self.local_core,
                )
                .await?
            }
            None => {
                StorageManager::from_config_allow_locked(current.clone(), self.local_core).await?
            }
        };
        let target_encrypted = password.is_some() || vault_password_from_env();
        let target_token_store = self
            .core
            .as_ref()
            .filter(|_| target.storage.backend.is_sqlite() && !target_encrypted)
            .map(|core| core.token_store.clone());
        let event_bus = self.core.as_ref().map(|core| core.event_bus.clone());
        let orchestrator =
            MigrationOrchestrator::from_manager(&manager, event_bus, target_token_store).await;

        let options = MigrationOptions {
            migrate_tokens: true,
            migrate_cache: true,
            token_password: password,
            ..MigrationOptions::default()
        };
        let plan = orchestrator.plan_migration(target.clone(), &options)?;
        let result = orchestrator
            .execute_migration(plan, options, self.local_core)
            .await;

        if result.success {
            return Ok(result.stats);
        }

        let message = match result.errors.join(", ") {
            joined if joined.is_empty() => "Transfer failed".to_string(),
            joined => joined,
        };
        Err(DomainError::DatabaseError(message))
    }

    async fn save(&self, config: &PipedashConfig) -> DomainResult<()> {
        if let Some(parent) = self.config_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                DomainError::InvalidConfig(format!("Failed to create config directory: {e}"))
            })?;
        }
        tokio::fs::create_dir_all(config.data_dir())
            .await
            .map_err(|e| {
                DomainError::InvalidConfig(format!("Failed to create data directory: {e}"))
            })?;

        let (config, path) = (config.clone(), self.config_path.clone());
        tokio::task::spawn_blocking(move || ConfigLoader::save(&config, &path))
            .await
            .map_err(internal)?
            .map_err(|e| DomainError::InvalidConfig(format!("Failed to save config: {e}")))?;

        tracing::info!("Setup config saved at {:?}", self.config_path);
        Ok(())
    }
}

fn current_data_dir<'a>(current: &'a PipedashConfig, default_dir: &'a str) -> &'a str {
    Some(current.storage.data_dir.as_str())
        .filter(|dir| !dir.is_empty())
        .unwrap_or(default_dir)
}

#[cfg(test)]
mod tests;
