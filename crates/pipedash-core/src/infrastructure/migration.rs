pub mod backup;
pub mod validator;

use std::collections::HashMap;
use std::sync::Arc;

pub use backup::{
    BackupManager,
    BackupPaths,
};
use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
pub use validator::{
    PreMigrationValidationReport,
    PreMigrationValidator,
};

use crate::domain::{
    DomainError,
    DomainResult,
    ProviderConfig,
};
use crate::event::EventBus;
use crate::infrastructure::config::{
    PipedashConfig,
    StorageConfig,
    StorageManager,
};
use crate::infrastructure::token_store::TokenStore;
use crate::infrastructure::{
    ConfigBackend,
    StorageBackend,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationStep {
    ValidateTarget,
    MigrateTokens,
    MigrateConfigs,
    MigrateCache,
    VerifyMigration,
    UpdateConfig,
}

impl MigrationStep {
    pub fn description(&self) -> &str {
        match self {
            MigrationStep::ValidateTarget => "Validating target configuration",
            MigrationStep::MigrateTokens => "Migrating tokens",
            MigrationStep::MigrateConfigs => "Migrating provider configurations",
            MigrationStep::MigrateCache => "Migrating cached data",
            MigrationStep::VerifyMigration => "Verifying migration",
            MigrationStep::UpdateConfig => "Updating storage configuration",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPlan {
    pub from: PipedashConfig,
    pub to: PipedashConfig,
    pub steps: Vec<MigrationStep>,
    pub migrate_tokens: bool,
    pub migrate_configs: bool,
    pub migrate_cache: bool,
    pub backend_changed: bool,
    pub data_dir_changed: bool,
    pub created_at: DateTime<Utc>,
}

impl MigrationPlan {
    pub fn summary(&self) -> String {
        format!(
            "Migrate from {} to {} ({} steps)",
            self.from.storage.summary(),
            self.to.storage.summary(),
            self.steps.len()
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationProgress {
    pub current_step: MigrationStep,
    pub step_index: usize,
    pub total_steps: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationResult {
    pub success: bool,
    pub steps_completed: Vec<MigrationStep>,
    pub errors: Vec<String>,
    pub duration_ms: u64,
    pub stats: MigrationStats,
    #[serde(default)]
    pub provider_id_mapping: std::collections::HashMap<i64, i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backups: Option<BackupPaths>,
}

impl MigrationResult {
    pub fn summary(&self) -> String {
        if self.success {
            format!(
                "Migration successful: {} providers, {} cache entries migrated in {}ms",
                self.stats.providers_migrated, self.stats.cache_entries_migrated, self.duration_ms
            )
        } else {
            format!(
                "Migration failed after {} steps: {}",
                self.steps_completed.len(),
                self.errors.join(", ")
            )
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MigrationStats {
    pub providers_migrated: usize,
    pub tokens_migrated: usize,
    pub cache_entries_migrated: usize,
    pub permissions_migrated: usize,
    pub providers_cleaned: usize,
    pub tokens_cleaned: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationOptions {
    pub migrate_tokens: bool,
    pub migrate_cache: bool,
    pub token_password: Option<String>,
    pub dry_run: bool,
    #[serde(default)]
    pub allow_non_empty_target: bool,
    #[serde(default)]
    pub clean_target: bool,
    #[serde(default)]
    pub keep_backups: bool,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_max_retries() -> u32 {
    3
}

impl Default for MigrationOptions {
    fn default() -> Self {
        Self {
            migrate_tokens: false,
            migrate_cache: false,
            token_password: None,
            dry_run: false,
            allow_non_empty_target: false,
            clean_target: false,
            keep_backups: false,
            max_retries: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub recommendations: Vec<String>,
}

impl ValidationReport {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

pub const VAULT_PASSWORD_ENV_VAR: &str = "PIPEDASH_VAULT_PASSWORD";

fn get_vault_password(options: &MigrationOptions) -> DomainResult<String> {
    if let Ok(password) = std::env::var(VAULT_PASSWORD_ENV_VAR) {
        if !password.is_empty() {
            return Ok(password);
        }
    }

    options.token_password.clone().ok_or_else(|| {
        DomainError::InvalidConfig(format!(
            "Vault password required for token migration. Set {} environment variable or provide token_password in options.",
            VAULT_PASSWORD_ENV_VAR
        ))
    })
}

pub struct MigrationOrchestrator {
    source_config: PipedashConfig,
    source_token_store: Arc<dyn TokenStore>,
    source_config_backend: Arc<dyn ConfigBackend>,
    source_cache_backend: Arc<dyn StorageBackend>,
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

        let needs_data_migration = backend_changed || data_dir_changed || token_backend_changing;

        if target_storage.backend == ConfigStorageBackend::Postgres {
            if target_storage.data_dir.starts_with("postgresql://")
                || target_storage.data_dir.starts_with("postgres://")
            {
                tracing::warn!(
                    "Target data_dir contains PostgreSQL connection string: {}. \
                     Resetting to empty (will use default) - connection string should be in postgres.connection_string field.",
                    target_storage.data_dir
                );
                target_storage.data_dir = String::new(); // Use default data_dir
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

    pub async fn validate_target_config(
        &self, config: &StorageConfig,
    ) -> DomainResult<ValidationReport> {
        use crate::infrastructure::config::StorageBackend as ConfigStorageBackend;

        let mut report = ValidationReport {
            errors: Vec::new(),
            warnings: Vec::new(),
            recommendations: Vec::new(),
        };

        if config.backend == ConfigStorageBackend::Postgres {
            if config.postgres.connection_string.is_empty() {
                report.errors.push(
                    "PostgreSQL connection string required when using postgres backend".into(),
                );
            }

            let has_settings_password = config.vault_password.is_some();
            let has_env_password = std::env::var(VAULT_PASSWORD_ENV_VAR).is_ok();

            if !has_settings_password && !has_env_password {
                report.warnings.push(format!(
                    "Vault password not provided. Set {} environment variable or provide password during setup.",
                    VAULT_PASSWORD_ENV_VAR
                ));
            }
        }

        Ok(report)
    }

    pub async fn execute_migration(
        &self, plan: MigrationPlan, options: MigrationOptions, is_desktop: bool,
    ) -> MigrationResult {
        let start_time = std::time::Instant::now();
        let mut result = MigrationResult {
            success: false,
            steps_completed: Vec::new(),
            errors: Vec::new(),
            duration_ms: 0,
            stats: MigrationStats::default(),
            provider_id_mapping: std::collections::HashMap::new(),
            backups: None,
        };

        if options.dry_run {
            result.success = true;
            result.duration_ms = start_time.elapsed().as_millis() as u64;
            return result;
        }

        let backup_manager = BackupManager::new(plan.from.data_dir());
        let backups = match backup_manager
            .create_backups(&plan.from, &plan.to, Some(&*self.source_config_backend))
            .await
        {
            Ok(backups) => backups,
            Err(e) => {
                result.errors.push(format!(
                    "Failed to create backups: {}\n\n\
                    WARNING: Migration aborted - backup creation is required for safe migration.\n\
                    INFO: Your data is safe - no modifications were performed.\n\n\
                    Please ensure:\n\
                    1. Sufficient disk space (need ~2x source database size)\n\
                    2. Write permissions to data directory\n\
                    3. Source database is not locked by another process",
                    e
                ));
                result.duration_ms = start_time.elapsed().as_millis() as u64;
                return result;
            }
        };
        result.backups = Some(backups.clone());

        let target_manager = if let Some(token_store) = &self.target_token_store {
            match StorageManager::with_token_store(plan.to.clone(), token_store.clone(), is_desktop)
                .await
            {
                Ok(m) => m,
                Err(e) => {
                    result.errors.push(format!(
                        "Failed to create target manager with token store: {}\n\n\
                        INFO: Migration aborted before any changes were made.\n\
                        INFO: Your data is safe - no modifications were performed.",
                        e
                    ));
                    backup_manager.cleanup_backups(&backups, false).await.ok();
                    result.duration_ms = start_time.elapsed().as_millis() as u64;
                    return result;
                }
            }
        } else {
            match StorageManager::from_config(plan.to.clone(), false).await {
                Ok(m) => m,
                Err(e) => {
                    result.errors.push(format!(
                        "Failed to create target manager: {}\n\n\
                        INFO: Migration aborted before any changes were made.\n\
                        INFO: Your data is safe - no modifications were performed.",
                        e
                    ));
                    backup_manager.cleanup_backups(&backups, false).await.ok();
                    result.duration_ms = start_time.elapsed().as_millis() as u64;
                    return result;
                }
            }
        };

        for (idx, step) in plan.steps.iter().enumerate() {
            self.emit_progress(*step, idx, plan.steps.len()).await;

            let step_result = self
                .execute_step(*step, &plan, &options, &target_manager, &mut result)
                .await;

            match step_result {
                Ok(()) => {
                    result.steps_completed.push(*step);
                }
                Err(e) => {
                    let step_name = format!("{:?}", step);

                    tracing::error!(
                        "Migration failed at step {} - restoring from backups",
                        step_name
                    );

                    let recovery_instructions = match backup_manager.restore_backups(&backups).await
                    {
                        Ok(()) => {
                            tracing::info!("Successfully restored from backups");
                            format!(
                                "✓ Your data has been restored from backups.\n\
                                 ✓ Config file: {}\n\
                                 ✓ Database: {}\n\n\
                                 The migration failed but your original data is intact.\n\
                                 Please review the error below and try again after fixing the issue:\n\n\
                                 Error: {}",
                                backups.source_config.display(),
                                backups.source_db.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "N/A".to_string()),
                                e
                            )
                        }
                        Err(restore_err) => {
                            format!(
                                "CRITICAL ERROR: Migration failed AND backup restoration failed!\n\n\
                                 Original error: {}\n\
                                 Restore error: {}\n\n\
                                 MANUAL RECOVERY REQUIRED:\n\
                                 1. Restore config file from: {}\n\
                                 2. Restore database from: {}\n\
                                 3. Review logs for details\n\
                                 4. Contact support if needed",
                                e,
                                restore_err,
                                backups.source_config.display(),
                                backups.source_db.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| "N/A".to_string())
                            )
                        }
                    };

                    result.errors.push(format!(
                        "Migration failed at step '{}':\n{}\n\n{}",
                        step_name, e, recovery_instructions
                    ));

                    self.rollback(&result).await;
                    result.duration_ms = start_time.elapsed().as_millis() as u64;
                    return result;
                }
            }
        }

        result.success = true;
        result.duration_ms = start_time.elapsed().as_millis() as u64;

        if let Err(e) = backup_manager
            .cleanup_backups(&backups, options.keep_backups)
            .await
        {
            tracing::warn!("Failed to cleanup backups: {}", e);
        }

        result
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
                tracing::info!("[Migration] Starting token migration...");
                let password = get_vault_password(options)?;
                tracing::info!(
                    "[Migration] Vault password retrieved: {} chars",
                    password.len()
                );

                tracing::info!("[Migration] Exporting tokens from source token store...");
                let encrypted_blob = self.source_token_store.export_encrypted(&password).await?;
                tracing::info!(
                    "[Migration] Exported encrypted blob: {} bytes",
                    encrypted_blob.len()
                );

                if !result.provider_id_mapping.is_empty() {
                    tracing::info!(
                        "[Migration] Using provider ID remapping path ({} mappings)",
                        result.provider_id_mapping.len()
                    );

                    let source_tokens = self.source_token_store.get_all_tokens().await?;
                    tracing::info!(
                        "[Migration] Retrieved {} tokens from source",
                        source_tokens.len()
                    );

                    let mut migrated_count = 0;
                    for (old_id, token) in source_tokens {
                        if let Some(&new_id) = result.provider_id_mapping.get(&old_id) {
                            tracing::debug!(
                                "[Migration] Storing token for provider {} → {} (token length: {})",
                                old_id,
                                new_id,
                                token.len()
                            );
                            target_manager
                                .token_store()
                                .await
                                .store_token(new_id, &token)
                                .await?;
                            migrated_count += 1;
                            tracing::info!(
                                "[Migration] Successfully stored token for provider {} (remapped to {})",
                                old_id,
                                new_id
                            );
                        } else {
                            let source_providers =
                                self.source_config_backend.list_providers().await?;
                            return Err(DomainError::DataConsistency(format!(
                                "Cannot migrate token for provider ID {} - provider not found in remapping.\n\n\
                                This indicates data corruption or inconsistency.\n\n\
                                Source provider IDs: {:?}\n\
                                Remapping table: {:?}\n\n\
                                This token would be lost if migration continued.\n\
                                Please investigate why this provider's configuration wasn't migrated.",
                                old_id,
                                source_providers.iter().filter_map(|p| p.id).collect::<Vec<_>>(),
                                result.provider_id_mapping
                            )));
                        }
                    }
                    tracing::info!(
                        "[Migration] Migrated {} tokens via ID remapping",
                        migrated_count
                    );
                } else {
                    tracing::info!("[Migration] Using direct import path (no ID remapping)");
                    tracing::info!(
                        "[Migration] Importing encrypted blob ({} bytes) to target token store...",
                        encrypted_blob.len()
                    );
                    target_manager
                        .token_store()
                        .await
                        .import_encrypted(&encrypted_blob, &password)
                        .await?;
                    tracing::info!("[Migration] Direct import completed");
                }

                let count: usize = target_manager
                    .token_store()
                    .await
                    .get_all_tokens()
                    .await?
                    .len();
                tracing::info!(
                    "[Migration] Verification: {} tokens now in target database",
                    count
                );
                result.stats.tokens_migrated = count;
            }

            MigrationStep::MigrateConfigs => {
                tracing::info!(
                    "[Migration] Exporting providers from source database: {:?}",
                    self.source_config.db_path()
                );
                let export = self.source_config_backend.export_all().await?;
                result.stats.providers_migrated = export.providers.len();
                result.stats.permissions_migrated = export.permissions.len();

                tracing::info!(
                    "[Migration] Importing {} providers to target database: {:?}",
                    export.providers.len(),
                    plan.to.db_path()
                );
                target_manager.config_backend().import_all(&export).await?;

                let source_providers = self.source_config_backend.list_providers().await?;
                let target_providers: Vec<ProviderConfig> =
                    target_manager.config_backend().list_providers().await?;

                for source_provider in &source_providers {
                    if let Some(source_id) = source_provider.id {
                        if let Some(target_provider) = target_providers
                            .iter()
                            .find(|p| p.name == source_provider.name)
                        {
                            if let Some(target_id) = target_provider.id {
                                if source_id != target_id {
                                    result.provider_id_mapping.insert(source_id, target_id);
                                }
                            }
                        }
                    }
                }

                if options.clean_target {
                    let source_names: std::collections::HashSet<String> =
                        source_providers.iter().map(|p| p.name.clone()).collect();

                    let orphaned: Vec<_> = target_providers
                        .iter()
                        .filter(|p| !source_names.contains(&p.name))
                        .collect();

                    for orphan in &orphaned {
                        if let Some(id) = orphan.id {
                            tracing::info!(
                                "[Migration] Removing orphaned provider: {} (id={})",
                                orphan.name,
                                id
                            );
                            target_manager.config_backend().delete_provider(id).await?;
                            let _ = target_manager.token_store().await.delete_token(id).await;
                            result.stats.tokens_cleaned += 1;
                        }
                    }

                    if !orphaned.is_empty() {
                        result.stats.providers_cleaned = orphaned.len();
                        tracing::info!(
                            "[Migration] Cleaned {} orphaned providers from target",
                            orphaned.len()
                        );
                    }
                }
            }

            MigrationStep::MigrateCache => {
                let objects = self.source_cache_backend.list(None).await?;
                tracing::info!(
                    "[Migration] Migrating {} cache entries from {:?} to {:?}",
                    objects.len(),
                    self.source_config.data_dir().join("cache"),
                    plan.to.data_dir().join("cache")
                );
                for obj in objects {
                    let data = self.source_cache_backend.get(&obj.key).await?;
                    target_manager
                        .cache_backend()
                        .put(&obj.key, &data, obj.content_type.as_deref())
                        .await?;
                    result.stats.cache_entries_migrated += 1;
                }
            }

            MigrationStep::VerifyMigration => {
                let source_providers = self.source_config_backend.list_providers().await?;

                if plan.migrate_configs && !source_providers.is_empty() {
                    let target_providers: Vec<ProviderConfig> =
                        target_manager.config_backend().list_providers().await?;

                    for source_provider in &source_providers {
                        let found = target_providers
                            .iter()
                            .any(|t| t.name == source_provider.name);
                        if !found {
                            return Err(DomainError::DatabaseError(format!(
                                "Provider '{}' from source was not found in target after migration",
                                source_provider.name
                            )));
                        }
                    }

                    if options.clean_target {
                        if source_providers.len() != target_providers.len() {
                            return Err(DomainError::DatabaseError(format!(
                                "Provider count mismatch after cleanup: source={}, target={}",
                                source_providers.len(),
                                target_providers.len()
                            )));
                        }
                        tracing::info!(
                            "[Migration] Verified exact match: {} providers",
                            source_providers.len()
                        );
                    } else if target_providers.len() > source_providers.len() {
                        tracing::info!(
                            "[Migration] Merge complete: {} migrated, {} pre-existing kept (total: {})",
                            source_providers.len(),
                            target_providers.len() - source_providers.len(),
                            target_providers.len()
                        );
                    } else {
                        tracing::info!(
                            "[Migration] Verified {} providers migrated successfully",
                            source_providers.len()
                        );
                    }
                } else if plan.migrate_configs {
                    tracing::info!("Skipping provider verification: source database is empty");
                }

                if plan.migrate_tokens {
                    let source_tokens = self.source_token_store.get_all_tokens().await?;

                    if !source_tokens.is_empty() {
                        let target_tokens: HashMap<i64, String> =
                            target_manager.token_store().await.get_all_tokens().await?;

                        if options.clean_target {
                            if source_tokens.len() != target_tokens.len() {
                                return Err(DomainError::DatabaseError(format!(
                                    "Token count mismatch after cleanup: source={}, target={}",
                                    source_tokens.len(),
                                    target_tokens.len()
                                )));
                            }
                            tracing::info!(
                                "[Migration] Verified exact match: {} tokens",
                                source_tokens.len()
                            );
                        } else {
                            if target_tokens.len() < source_tokens.len() {
                                return Err(DomainError::DatabaseError(format!(
                                    "Token count too low after migration: source={}, target={}",
                                    source_tokens.len(),
                                    target_tokens.len()
                                )));
                            }
                            tracing::info!(
                                "[Migration] Token migration complete: {} migrated (target has {} total)",
                                source_tokens.len(),
                                target_tokens.len()
                            );
                        }
                    } else {
                        tracing::info!("Skipping token verification: source has no tokens");
                    }
                }
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

                ConfigLoader::save(&plan.to, &config_path).map_err(|e| {
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

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::infrastructure::token_store::MemoryTokenStore;

    async fn create_test_manager(temp_dir: &TempDir) -> StorageManager {
        let mut config = PipedashConfig::default();
        config.storage.data_dir = temp_dir.path().to_string_lossy().to_string();

        let token_store: Arc<dyn TokenStore> = Arc::new(MemoryTokenStore::new());
        StorageManager::with_token_store(config, token_store, true)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_plan_migration_no_changes() {
        let temp_dir = TempDir::new().unwrap();
        let manager = create_test_manager(&temp_dir).await;

        let orchestrator = MigrationOrchestrator::from_manager(&manager, None, None).await;

        let target_config = manager.config().clone();
        let plan = orchestrator
            .plan_migration(target_config, &MigrationOptions::default())
            .unwrap();

        assert!(!plan.steps.contains(&MigrationStep::MigrateConfigs));
        assert!(!plan.steps.contains(&MigrationStep::MigrateTokens));
    }

    #[tokio::test]
    async fn test_migration_dry_run() {
        let temp_dir = TempDir::new().unwrap();
        let manager = create_test_manager(&temp_dir).await;

        let orchestrator = MigrationOrchestrator::from_manager(&manager, None, None).await;

        let target_config = manager.config().clone();
        let plan = MigrationPlan {
            from: manager.config().clone(),
            to: target_config,
            steps: vec![MigrationStep::ValidateTarget],
            migrate_tokens: false,
            migrate_configs: false,
            migrate_cache: false,
            backend_changed: false,
            data_dir_changed: false,
            created_at: Utc::now(),
        };

        let result = orchestrator
            .execute_migration(
                plan,
                MigrationOptions {
                    dry_run: true,
                    ..Default::default()
                },
                true, // is_desktop
            )
            .await;

        assert!(result.success);
    }

    #[test]
    fn test_migration_step_descriptions() {
        assert!(!MigrationStep::ValidateTarget.description().is_empty());
        assert!(!MigrationStep::MigrateTokens.description().is_empty());
        assert!(!MigrationStep::MigrateConfigs.description().is_empty());
    }

    #[test]
    fn test_migration_result_summary() {
        let result = MigrationResult {
            success: true,
            steps_completed: vec![MigrationStep::MigrateConfigs],
            errors: vec![],
            duration_ms: 100,
            stats: MigrationStats {
                providers_migrated: 5,
                tokens_migrated: 3,
                cache_entries_migrated: 10,
                permissions_migrated: 2,
                providers_cleaned: 0,
                tokens_cleaned: 0,
            },
            provider_id_mapping: std::collections::HashMap::new(),
            backups: None,
        };

        let summary = result.summary();
        assert!(summary.contains("5 providers"));
        assert!(summary.contains("10 cache entries"));
    }
}
