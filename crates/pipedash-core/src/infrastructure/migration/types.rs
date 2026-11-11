use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::backup::BackupPaths;
use crate::infrastructure::config::PipedashConfig;

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
