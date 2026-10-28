pub mod backup;
pub mod validator;

use std::collections::{
    BTreeMap,
    HashMap,
};
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
    MetricsQuery,
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
    database::{
        DatabasePool,
        MetricsRepository,
        PipelineViewRepository,
        PushRepository,
        RecordMetrics,
    },
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
    pub metrics_migrated: usize,
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

fn remap_pipeline_id(pipeline_id: &str, provider_ids: &HashMap<i64, i64>) -> Option<String> {
    let mut parts = pipeline_id.splitn(3, "__");
    let (Some(provider_type), Some(provider_id), Some(rest)) =
        (parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    let new_id = provider_ids.get(&provider_id.parse().ok()?)?;

    Some(format!("{provider_type}__{new_id}__{rest}"))
}

fn same_database(from: &PipedashConfig, to: &PipedashConfig) -> bool {
    from.storage.backend == to.storage.backend
        && if to.storage.backend.is_sqlite() {
            from.db_path() == to.db_path()
        } else {
            from.storage.postgres.connection_string == to.storage.postgres.connection_string
        }
}

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
                                result.provider_id_mapping.insert(source_id, target_id);
                            }
                        }
                    }
                }

                if !same_database(&plan.from, &plan.to) {
                    let mut metrics_config = self.source_metrics.export_config().await?;
                    metrics_config.pipeline_configs.retain_mut(|config| {
                        match remap_pipeline_id(&config.pipeline_id, &result.provider_id_mapping) {
                            Some(pipeline_id) => {
                                config.pipeline_id = pipeline_id;
                                true
                            }
                            None => false,
                        }
                    });
                    MetricsRepository::new_from_pool(target_manager.database_pool())
                        .import_config(&metrics_config)
                        .await?;
                }

                if !same_database(&plan.from, &plan.to) {
                    let subscriptions =
                        PushRepository::new(self.source_pool.clone()).list().await?;
                    let target_push = PushRepository::new(target_manager.database_pool());
                    target_push.import(&subscriptions).await?;
                    if options.clean_target {
                        for stale in target_push.list().await? {
                            if !subscriptions.iter().any(|s| s.id == stale.id) {
                                target_push.delete(&stale.id).await?;
                            }
                        }
                    }
                    tracing::info!(
                        "[Migration] Migrated {} push subscriptions",
                        subscriptions.len()
                    );
                }

                if !same_database(&plan.from, &plan.to) {
                    let mapping = &result.provider_id_mapping;
                    let mut export = PipelineViewRepository::new(self.source_pool.clone())
                        .export()
                        .await?;
                    let remap = |pipeline_id: &mut String, provider_id: &mut i64| match (
                        remap_pipeline_id(pipeline_id, mapping),
                        mapping.get(provider_id),
                    ) {
                        (Some(next), Some(target)) => {
                            *pipeline_id = next;
                            *provider_id = *target;
                            true
                        }
                        _ => false,
                    };
                    export
                        .members
                        .retain_mut(|m| remap(&mut m.pipeline_id, &mut m.provider_id));
                    for view in &mut export.views {
                        view.rules.provider_ids = view
                            .rules
                            .provider_ids
                            .iter()
                            .filter_map(|id| mapping.get(id).copied())
                            .collect();
                    }
                    PipelineViewRepository::new(target_manager.database_pool())
                        .import(&export, options.clean_target)
                        .await?;
                    tracing::info!("[Migration] Migrated {} pipeline views", export.views.len());
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

                if !same_database(&plan.from, &plan.to) {
                    let target_metrics =
                        MetricsRepository::new_from_pool(target_manager.database_pool());
                    let pipeline_query = |pipeline_id: &str| MetricsQuery {
                        pipeline_id: Some(pipeline_id.to_string()),
                        limit: None,
                        ..Default::default()
                    };
                    let mut pipelines = 0;
                    for source_id in self.source_metrics.list_pipeline_ids().await? {
                        let Some(target_id) =
                            remap_pipeline_id(&source_id, &result.provider_id_mapping)
                        else {
                            continue;
                        };
                        let last_run = self
                            .source_metrics
                            .get_last_processed_run(&source_id)
                            .await?;
                        let mut runs: BTreeMap<i64, RecordMetrics> = BTreeMap::new();
                        for entry in self
                            .source_metrics
                            .query_metrics(pipeline_query(&source_id))
                            .await?
                        {
                            let slot = entry.metric_type as usize;
                            let record =
                                runs.entry(entry.run_number)
                                    .or_insert_with(|| RecordMetrics {
                                        run_number: entry.run_number,
                                        run_hash: entry.run_hash.clone().unwrap_or_default(),
                                        entries: [None, None, None],
                                    });
                            record.entries[slot] = Some(entry);
                        }
                        let records: Vec<_> = runs.into_values().collect();
                        target_metrics
                            .refresh_record_metrics(&target_id, &records)
                            .await?;
                        result.stats.metrics_migrated += records
                            .iter()
                            .map(|record| record.entries.iter().flatten().count())
                            .sum::<usize>();
                        pipelines += 1;

                        let Some(run) = last_run else {
                            continue;
                        };
                        if target_metrics
                            .get_last_processed_run(&target_id)
                            .await?
                            .is_none_or(|target_run| target_run < run)
                        {
                            target_metrics
                                .update_last_processed_run(&target_id, run)
                                .await?;
                        }
                    }
                    tracing::info!(
                        "[Migration] Migrated {} metric entries for {} pipelines",
                        result.stats.metrics_migrated,
                        pipelines
                    );
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
    use crate::{
        infrastructure::token_store::MemoryTokenStore,
        test_env::EnvGuard,
    };

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
                metrics_migrated: 0,
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

    #[tokio::test]
    async fn migrate_tokens_after_a_provider_was_deleted() {
        let source_dir = TempDir::new().unwrap();
        let target_dir = TempDir::new().unwrap();
        let config_path = source_dir.path().join("config.toml");
        std::fs::write(&config_path, "[storage]\nbackend = \"sqlite\"\n").unwrap();
        let _env = EnvGuard::set(&[("PIPEDASH_CONFIG_PATH", config_path.to_str())]);

        let mut source = PipedashConfig::default();
        source.storage.data_dir = source_dir.path().to_string_lossy().to_string();
        source.storage.vault_password = Some("test-password".to_string());
        let mut target = source.clone();
        target.storage.data_dir = target_dir.path().to_string_lossy().to_string();

        let manager = StorageManager::from_config(source, false).await.unwrap();
        let backend = manager.config_backend();
        let tokens = manager.token_store().await;
        let mut ids = Vec::new();
        for name in ["a", "b", "c"] {
            let provider = ProviderConfig {
                id: None,
                name: name.to_string(),
                provider_type: "github".to_string(),
                token: String::new(),
                config: HashMap::new(),
                refresh_interval: 30,
                version: None,
            };
            ids.push(backend.create_provider(&provider).await.unwrap());
        }
        backend.delete_provider(ids[1]).await.unwrap();
        tokens.store_token(ids[0], "tok-a").await.unwrap();
        tokens.store_token(ids[2], "tok-c").await.unwrap();

        let orchestrator = MigrationOrchestrator::from_manager(&manager, None, None).await;
        let options = MigrationOptions {
            migrate_tokens: true,
            token_password: Some("test-password".to_string()),
            ..Default::default()
        };
        let plan = orchestrator
            .plan_migration(target.clone(), &options)
            .unwrap();
        let result = orchestrator.execute_migration(plan, options, false).await;
        assert!(result.success, "{:?}", result.errors);

        let target_manager = StorageManager::from_config(target, false).await.unwrap();
        let mut migrated = HashMap::new();
        for provider in target_manager
            .config_backend()
            .list_providers()
            .await
            .unwrap()
        {
            let token = target_manager
                .token_store()
                .await
                .get_token(provider.id.unwrap())
                .await
                .unwrap();
            migrated.insert(provider.name, token);
        }
        assert_eq!(
            migrated,
            HashMap::from([
                ("a".to_string(), "tok-a".to_string()),
                ("c".to_string(), "tok-c".to_string()),
            ])
        );
    }

    #[tokio::test]
    async fn migration_keeps_pipeline_metrics() {
        use crate::domain::{
            MetricEntry,
            MetricType,
        };
        use crate::infrastructure::database::RecordMetrics;

        let source_dir = TempDir::new().unwrap();
        let target_dir = TempDir::new().unwrap();
        let config_path = source_dir.path().join("config.toml");
        std::fs::write(&config_path, "[storage]\nbackend = \"sqlite\"\n").unwrap();
        let _env = EnvGuard::set(&[("PIPEDASH_CONFIG_PATH", config_path.to_str())]);

        let mut source = PipedashConfig::default();
        source.storage.data_dir = source_dir.path().to_string_lossy().to_string();
        source.storage.vault_password = Some("test-password".to_string());
        let mut target = source.clone();
        target.storage.data_dir = target_dir.path().to_string_lossy().to_string();

        let manager = StorageManager::from_config(source, false).await.unwrap();
        let backend = manager.config_backend();
        let mut ids = Vec::new();
        for name in ["deleted", "gh"] {
            let provider = ProviderConfig {
                id: None,
                name: name.to_string(),
                provider_type: "github".to_string(),
                token: String::new(),
                config: HashMap::new(),
                refresh_interval: 30,
                version: None,
            };
            ids.push(backend.create_provider(&provider).await.unwrap());
        }
        backend.delete_provider(ids[0]).await.unwrap();
        let orphan_pipeline = format!("github__{}__o__r__1", ids[0]);
        let source_pipeline = format!("github__{}__o__r__1", ids[1]);
        let expired_pipeline = format!("github__{}__o__r__2", ids[1]);

        let metrics = MetricsRepository::new_from_pool(manager.database_pool());
        metrics.update_global_config(true, 30).await.unwrap();
        metrics
            .upsert_pipeline_config(&source_pipeline, true, 14)
            .await
            .unwrap();
        metrics
            .upsert_pipeline_config(&orphan_pipeline, false, 3)
            .await
            .unwrap();
        let entry = |pipeline_id: &str, n: i64| MetricEntry {
            id: 0,
            pipeline_id: pipeline_id.to_string(),
            run_number: n,
            timestamp: Utc::now(),
            metric_type: MetricType::RunDuration,
            value: 60.0 * n as f64,
            metadata: None,
            created_at: Utc::now(),
            run_hash: Some(format!("hash-{n}")),
        };
        let mut entries: Vec<MetricEntry> = (1..=5).map(|n| entry(&source_pipeline, n)).collect();
        entries.push(entry(&orphan_pipeline, 1));
        metrics.insert_metrics_batch(entries).await.unwrap();
        metrics
            .update_last_processed_run(&source_pipeline, 5)
            .await
            .unwrap();
        metrics
            .update_last_processed_run(&expired_pipeline, 9)
            .await
            .unwrap();

        let orchestrator = MigrationOrchestrator::from_manager(&manager, None, None).await;
        let options = MigrationOptions {
            migrate_tokens: true,
            migrate_cache: true,
            token_password: Some("test-password".to_string()),
            ..Default::default()
        };
        let plan = orchestrator
            .plan_migration(target.clone(), &options)
            .unwrap();
        let migrate = || orchestrator.execute_migration(plan.clone(), options.clone(), false);
        let result = migrate().await;
        assert!(result.success, "{:?}", result.errors);
        assert_eq!(result.stats.metrics_migrated, 5);
        let refreshed = MetricEntry {
            run_hash: Some("hash-4b".to_string()),
            value: 999.0,
            ..entry(&source_pipeline, 4)
        };
        metrics
            .refresh_record_metrics(
                &source_pipeline,
                &[RecordMetrics {
                    run_number: 4,
                    run_hash: "hash-4b".to_string(),
                    entries: [Some(refreshed), None, None],
                }],
            )
            .await
            .unwrap();
        let result = migrate().await;
        assert!(result.success, "{:?}", result.errors);

        let target_manager = StorageManager::from_config(target, false).await.unwrap();
        let target_id = target_manager
            .config_backend()
            .list_providers()
            .await
            .unwrap()[0]
            .id
            .unwrap();
        assert_ne!(target_id, ids[1]);
        let target_pipeline = format!("github__{target_id}__o__r__1");
        let target_metrics = MetricsRepository::new_from_pool(target_manager.database_pool());

        let copied = target_metrics
            .query_metrics(MetricsQuery {
                pipeline_id: Some(target_pipeline.clone()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(copied.len(), 5);
        let run_4: Vec<_> = copied
            .into_iter()
            .filter(|entry| entry.run_number == 4)
            .map(|entry| (entry.run_hash, entry.value))
            .collect();
        assert_eq!(run_4, [(Some("hash-4b".to_string()), 999.0)]);
        let config = target_metrics
            .get_pipeline_config(&target_pipeline)
            .await
            .unwrap()
            .unwrap();
        assert!(config.enabled);
        assert_eq!(config.retention_days, 14);
        let global = target_metrics.get_global_config().await.unwrap();
        assert!(global.enabled);
        assert_eq!(global.default_retention_days, 30);
        assert_eq!(
            target_metrics
                .get_last_processed_run(&target_pipeline)
                .await
                .unwrap(),
            Some(5)
        );
        assert_eq!(
            target_metrics
                .get_last_processed_run(&format!("github__{target_id}__o__r__2"))
                .await
                .unwrap(),
            Some(9)
        );
        assert_eq!(
            target_metrics
                .export_config()
                .await
                .unwrap()
                .pipeline_configs
                .len(),
            1
        );
        assert_eq!(
            target_metrics
                .calculate_storage_stats()
                .await
                .unwrap()
                .total_metrics_count,
            5
        );
    }

    #[tokio::test]
    async fn push_subscriptions_follow_a_storage_migration_without_the_cache() {
        let source_dir = TempDir::new().unwrap();
        let target_dir = TempDir::new().unwrap();
        let config_path = source_dir.path().join("config.toml");
        std::fs::write(&config_path, "[storage]\nbackend = \"sqlite\"\n").unwrap();
        let _env = EnvGuard::set(&[("PIPEDASH_CONFIG_PATH", config_path.to_str())]);

        let mut source = PipedashConfig::default();
        source.storage.data_dir = source_dir.path().to_string_lossy().to_string();
        source.storage.vault_password = Some("test-password".to_string());
        let mut target = source.clone();
        target.storage.data_dir = target_dir.path().to_string_lossy().to_string();

        let manager = StorageManager::from_config(source, false).await.unwrap();
        let push = PushRepository::new(manager.database_pool());
        push.register("relay-a", "iPhone", "ios").await.unwrap();
        let paired = push.register("relay-b", "iPad", "ios").await.unwrap();
        push.mark_success(&paired.id).await.unwrap();
        let expected = push.list().await.unwrap();

        let orchestrator = MigrationOrchestrator::from_manager(&manager, None, None).await;
        let options = MigrationOptions {
            migrate_tokens: false,
            migrate_cache: false,
            ..Default::default()
        };
        let plan = orchestrator
            .plan_migration(target.clone(), &options)
            .unwrap();
        let result = orchestrator.execute_migration(plan, options, false).await;
        assert!(result.success, "{:?}", result.errors);

        let target_manager = StorageManager::from_config(target, false).await.unwrap();
        let migrated = PushRepository::new(target_manager.database_pool())
            .list()
            .await
            .unwrap();
        assert_eq!(migrated, expected);
    }

    #[tokio::test]
    async fn views_and_pins_survive_a_restart_and_a_storage_migration() {
        use crate::domain::{
            PinKind,
            SectionMode,
            ViewDisplay,
            ViewKind,
            ViewLayout,
            ViewRules,
        };
        use crate::infrastructure::database::ViewExport;

        let source_dir = TempDir::new().unwrap();
        let target_dir = TempDir::new().unwrap();
        let config_path = source_dir.path().join("config.toml");
        std::fs::write(&config_path, "[storage]\nbackend = \"sqlite\"\n").unwrap();
        let _env = EnvGuard::set(&[("PIPEDASH_CONFIG_PATH", config_path.to_str())]);

        let mut source = PipedashConfig::default();
        source.storage.data_dir = source_dir.path().to_string_lossy().to_string();
        source.storage.vault_password = Some("test-password".to_string());
        let mut target = source.clone();
        target.storage.data_dir = target_dir.path().to_string_lossy().to_string();

        let manager = StorageManager::from_config(source.clone(), false)
            .await
            .unwrap();
        let backend = manager.config_backend();
        let mut ids = Vec::new();
        for name in ["deleted", "gh"] {
            let provider = ProviderConfig {
                id: None,
                name: name.to_string(),
                provider_type: "github".to_string(),
                token: String::new(),
                config: HashMap::new(),
                refresh_interval: 30,
                version: None,
            };
            ids.push(backend.create_provider(&provider).await.unwrap());
        }
        backend.delete_provider(ids[0]).await.unwrap();
        let provider = ids[1];
        let pipeline = |n: u32| format!("github__{provider}__o__r__{n}");

        let repo = PipelineViewRepository::new(manager.database_pool());
        let rules = ViewRules {
            provider_ids: vec![provider],
            repositories: vec!["o/*".to_string()],
            name_contains: Some("deploy".to_string()),
            statuses: Vec::new(),
        };
        let cards = ViewDisplay {
            layout: ViewLayout::Cards,
            sections: SectionMode::Provider,
            ..ViewDisplay::default()
        };
        let release = repo
            .insert_custom("Release", None, &rules, &cards)
            .await
            .unwrap();
        let prod = repo
            .insert_custom(
                "Prod",
                Some(release),
                &ViewRules::default(),
                &ViewDisplay::default(),
            )
            .await
            .unwrap();
        let eu = repo
            .insert_custom(
                "EU",
                Some(prod),
                &ViewRules::default(),
                &ViewDisplay::default(),
            )
            .await
            .unwrap();
        repo.upsert_member(eu, provider, &pipeline(1), false)
            .await
            .unwrap();
        repo.upsert_member(release, provider, &pipeline(2), true)
            .await
            .unwrap();
        let pinned = repo
            .list_views()
            .await
            .unwrap()
            .into_iter()
            .find(|v| v.kind == ViewKind::Pinned)
            .unwrap();
        repo.update_view(
            pinned.id,
            &pinned.name,
            None,
            &pinned.rules,
            &ViewDisplay {
                layout: ViewLayout::Table,
                ..pinned.display
            },
        )
        .await
        .unwrap();
        repo.insert_pin(PinKind::View, &eu.to_string())
            .await
            .unwrap();
        repo.insert_pin(PinKind::Repository, "o/r").await.unwrap();
        repo.insert_pin(PinKind::View, &release.to_string())
            .await
            .unwrap();

        let snapshot = |export: ViewExport, provider: i64| {
            let pid = |id: &str| id.replace(&format!("__{provider}__"), "__P__");
            let view_name = |id: i64| {
                export
                    .views
                    .iter()
                    .find(|v| v.id == id)
                    .unwrap()
                    .name
                    .clone()
            };
            let views: Vec<_> = export
                .views
                .iter()
                .map(|v| {
                    let mut rules = v.rules.clone();
                    rules.provider_ids = rules
                        .provider_ids
                        .iter()
                        .map(|id| i64::from(*id == provider))
                        .collect();
                    (
                        v.kind,
                        v.name.clone(),
                        v.parent_id.map(view_name),
                        v.position,
                        rules,
                        v.display,
                    )
                })
                .collect();
            let members: Vec<_> = export
                .members
                .iter()
                .map(|m| {
                    (
                        view_name(m.view_id),
                        pid(&m.pipeline_id),
                        m.excluded,
                        m.provider_id == provider,
                    )
                })
                .collect();
            let pins: Vec<_> = export
                .pins
                .iter()
                .map(|p| match p.kind {
                    PinKind::View => view_name(p.reference.parse().unwrap()),
                    PinKind::Repository => p.reference.clone(),
                })
                .collect();
            format!("{views:?}\n{members:?}\n{pins:?}")
        };
        let before = snapshot(repo.export().await.unwrap(), provider);
        drop(repo);
        drop(manager);

        let manager = StorageManager::from_config(source, false).await.unwrap();
        let reopened = PipelineViewRepository::new(manager.database_pool())
            .export()
            .await
            .unwrap();
        assert_eq!(snapshot(reopened, provider), before);

        let orchestrator = MigrationOrchestrator::from_manager(&manager, None, None).await;
        let options = MigrationOptions {
            migrate_tokens: false,
            migrate_cache: false,
            ..Default::default()
        };
        let plan = orchestrator
            .plan_migration(target.clone(), &options)
            .unwrap();
        let result = orchestrator.execute_migration(plan, options, false).await;
        assert!(result.success, "{:?}", result.errors);

        let target_manager = StorageManager::from_config(target, false).await.unwrap();
        let target_provider = target_manager
            .config_backend()
            .list_providers()
            .await
            .unwrap()[0]
            .id
            .unwrap();
        assert_ne!(target_provider, provider);
        let migrated = PipelineViewRepository::new(target_manager.database_pool())
            .export()
            .await
            .unwrap();
        assert_eq!(snapshot(migrated, target_provider), before);
    }
}
