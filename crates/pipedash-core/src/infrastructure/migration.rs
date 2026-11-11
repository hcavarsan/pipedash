pub mod backup;

mod alerts;
mod cache;
mod configs;
mod devices;
mod execution;
mod metrics;
pub mod remap;
#[cfg(test)]
mod tests;
mod tokens;
mod types;
mod verify;
mod views;

use std::sync::Arc;

use chrono::Utc;
pub use types::{
    MigrationOptions,
    MigrationPlan,
    MigrationResult,
    MigrationStats,
    MigrationStep,
};

use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::event::EventBus;
use crate::infrastructure::config::{
    PipedashConfig,
    StorageManager,
};
use crate::infrastructure::token_store::TokenStore;
use crate::infrastructure::{
    database::{
        DatabasePool,
        MetricsRepository,
    },
    ConfigBackend,
    StorageBackend,
};

pub const VAULT_PASSWORD_ENV_VAR: &str = "PIPEDASH_VAULT_PASSWORD";

pub struct MigrationOrchestrator {
    source_config: PipedashConfig,
    source_token_store: Arc<dyn TokenStore>,
    source_config_backend: Arc<dyn ConfigBackend>,
    source_cache_backend: Arc<dyn StorageBackend>,
    source_metrics: MetricsRepository,
    source_pool: DatabasePool,
    event_bus: Option<Arc<dyn EventBus>>,
    target_token_store: Option<Arc<dyn TokenStore>>,
}

impl MigrationOrchestrator {
    pub async fn from_manager(
        manager: &StorageManager, event_bus: Option<Arc<dyn EventBus>>,
        target_token_store: Option<Arc<dyn TokenStore>>,
    ) -> Self {
        Self {
            source_config: manager.config().clone(),
            source_token_store: manager.token_store().await,
            source_config_backend: manager.config_backend(),
            source_cache_backend: manager.cache_backend(),
            source_metrics: MetricsRepository::new_from_pool(manager.database_pool()),
            source_pool: manager.database_pool(),
            event_bus,
            target_token_store,
        }
    }

    pub fn plan_migration(
        &self, mut target_config: PipedashConfig, options: &MigrationOptions,
    ) -> DomainResult<MigrationPlan> {
        use crate::infrastructure::config::StorageBackend as ConfigStorageBackend;

        let source_data_dir = self.source_config.data_dir();
        let target_data_dir = target_config.data_dir();
        let data_dir_changed = source_data_dir != target_data_dir;
        let database_changed = !remap::same_database(&self.source_config, &target_config);

        let source_storage = &self.source_config.storage;
        let target_storage = &mut target_config.storage;

        if data_dir_changed {
            tracing::info!(
                "[Migration Planning] Data directory change detected: {:?} → {:?}",
                source_data_dir,
                target_data_dir
            );
        }

        let backend_changed = source_storage.backend != target_storage.backend;

        let token_backend_changing = {
            let source_uses_keyring = source_storage.backend.is_sqlite()
                && std::env::var(VAULT_PASSWORD_ENV_VAR).is_err();
            let target_uses_encrypted = target_storage.backend.is_sqlite()
                && (options.token_password.is_some()
                    || std::env::var(VAULT_PASSWORD_ENV_VAR).is_ok());

            source_uses_keyring && target_uses_encrypted
        };

        if token_backend_changing {
            tracing::info!(
                "[Migration Planning] Token backend change detected: keyring → encrypted storage"
            );
        }

        let needs_data_migration = database_changed || data_dir_changed || token_backend_changing;

        if target_storage.backend == ConfigStorageBackend::Postgres {
            if target_storage.data_dir.starts_with("postgresql://")
                || target_storage.data_dir.starts_with("postgres://")
            {
                tracing::warn!(
                    "Target data_dir contains PostgreSQL connection string: {}. \
                     Resetting to empty (will use default) - connection string should be in postgres.connection_string field.",
                    target_storage.data_dir
                );
                target_storage.data_dir = String::new();
            } else if target_storage.data_dir.is_empty() {
                tracing::debug!("Target uses PostgreSQL - data_dir is empty (will use default for cache/local files)");
            }
        }

        let mut steps = Vec::new();

        steps.push(MigrationStep::ValidateTarget);

        if needs_data_migration {
            steps.push(MigrationStep::MigrateConfigs);
        }

        if needs_data_migration && options.migrate_tokens {
            steps.push(MigrationStep::MigrateTokens);
        }

        if needs_data_migration && options.migrate_cache {
            steps.push(MigrationStep::MigrateCache);
        }

        if !steps.is_empty() {
            steps.push(MigrationStep::VerifyMigration);
            steps.push(MigrationStep::UpdateConfig);
        }

        Ok(MigrationPlan {
            from: self.source_config.clone(),
            to: target_config,
            steps,
            migrate_tokens: needs_data_migration && options.migrate_tokens,
            migrate_configs: needs_data_migration,
            migrate_cache: needs_data_migration && options.migrate_cache,
            backend_changed,
            data_dir_changed,
            created_at: Utc::now(),
        })
    }

    async fn execute_step(
        &self, step: MigrationStep, plan: &MigrationPlan, options: &MigrationOptions,
        target_manager: &StorageManager, result: &mut MigrationResult,
    ) -> DomainResult<()> {
        match step {
            MigrationStep::ValidateTarget => {
                if !target_manager.cache_backend().is_available().await {
                    return Err(DomainError::NetworkError(
                        "Target cache backend not available".into(),
                    ));
                }

                target_manager.config_backend().list_providers().await?;
            }
            MigrationStep::MigrateTokens => {
                self.migrate_tokens(options, target_manager, result).await?
            }
            MigrationStep::MigrateConfigs => {
                self.migrate_configs(plan, options, target_manager, result)
                    .await?
            }
            MigrationStep::MigrateCache => self.migrate_cache(plan, target_manager, result).await?,
            MigrationStep::VerifyMigration => {
                self.verify_migration(plan, options, target_manager).await?
            }
            MigrationStep::UpdateConfig => {
                use crate::infrastructure::config::ConfigLoader;

                let config_path = ConfigLoader::discover_config_path();

                tracing::info!(
                    "Saving migrated config to: {} (data_dir: {:?}, backend: {})",
                    config_path.display(),
                    plan.to.data_dir(),
                    plan.to.storage.backend
                );

                let config = plan.to.clone();
                tokio::task::spawn_blocking(move || ConfigLoader::save(&config, &config_path))
                    .await
                    .map_err(|e| {
                        DomainError::DatabaseError(format!("Failed to save config: {}", e))
                    })?
                    .map_err(|e| {
                        DomainError::DatabaseError(format!("Failed to save config: {}", e))
                    })?;
            }
        }

        Ok(())
    }

    async fn emit_progress(&self, step: MigrationStep, step_index: usize, total_steps: usize) {
        if let Some(ref event_bus) = self.event_bus {
            event_bus
                .emit(crate::event::CoreEvent::MigrationProgress {
                    step: format!("{:?}", step),
                    step_index,
                    total_steps,
                    message: step.description().to_string(),
                })
                .await;
        }
    }

    async fn rollback(&self, _result: &MigrationResult) {
        tracing::warn!("Migration failed. Manual intervention may be required.");
    }
}
