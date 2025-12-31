use std::collections::{
    HashMap,
    HashSet,
};
use std::path::Path;

use super::MigrationOptions;
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::config::{
    PipedashConfig,
    StorageBackend,
};
use crate::infrastructure::StorageManager;

#[derive(Debug, Clone)]
pub struct PreMigrationValidationReport {
    pub passed: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub estimated_backup_size: u64,
}

impl PreMigrationValidationReport {
    pub fn new() -> Self {
        Self {
            passed: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            estimated_backup_size: 0,
        }
    }

    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
        self.passed = false;
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    pub fn into_result(self) -> DomainResult<Self> {
        if self.passed {
            Ok(self)
        } else {
            Err(DomainError::InvalidConfig(format!(
                "Pre-migration validation failed:\n{}",
                self.errors.join("\n")
            )))
        }
    }
}

impl Default for PreMigrationValidationReport {
    fn default() -> Self {
        Self::new()
    }
}

pub struct PreMigrationValidator;

impl PreMigrationValidator {
    pub async fn validate_all(
        source_config: &PipedashConfig, target_config: &PipedashConfig,
        source_manager: &StorageManager, target_manager: &StorageManager,
        options: &MigrationOptions,
    ) -> DomainResult<PreMigrationValidationReport> {
        let mut report = PreMigrationValidationReport::new();

        Self::validate_disk_space(source_config, &mut report).await;

        Self::validate_provider_names(source_manager, &mut report).await;

        Self::validate_no_orphaned_tokens(source_manager, &mut report).await;

        Self::validate_target_state(target_manager, options, &mut report).await;

        if target_config.storage.backend == StorageBackend::Postgres {
            Self::validate_postgres_schema(target_config, &mut report).await;
        }

        report.into_result()
    }

    async fn validate_disk_space(
        source_config: &PipedashConfig, report: &mut PreMigrationValidationReport,
    ) {
        use crate::infrastructure::migration::backup::BackupManager;

        let backup_manager = BackupManager::new(source_config.data_dir());
        let estimated_size = backup_manager.estimate_backup_size(source_config).await;
        report.estimated_backup_size = estimated_size;

        let data_dir = source_config.data_dir();
        if let Ok(available) = Self::get_available_disk_space(&data_dir) {
            let required = estimated_size * 2; // Need 2x for safety margin

            if available < required {
                report.add_error(format!(
                    "Insufficient disk space for migration backup.\n\
                    Required: {} MB\n\
                    Available: {} MB\n\
                    Please free up disk space before continuing.",
                    required / 1_048_576,
                    available / 1_048_576
                ));
            } else if available < required * 2 {
                report.add_warning(format!(
                    "Low disk space detected.\n\
                    Required: {} MB\n\
                    Available: {} MB\n\
                    Consider freeing up space for safety.",
                    required / 1_048_576,
                    available / 1_048_576
                ));
            }
        } else {
            report.add_warning("Could not check disk space - proceeding with caution".to_string());
        }
    }

    fn get_available_disk_space(path: &Path) -> std::io::Result<u64> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = std::fs::metadata(path)?;
            Ok(metadata.blocks() * 512)
        }

        #[cfg(not(unix))]
        {
            let _ = path;
            Ok(u64::MAX)
        }
    }

    async fn validate_provider_names(
        source_manager: &StorageManager, report: &mut PreMigrationValidationReport,
    ) {
        match source_manager.config_backend().list_providers().await {
            Ok(providers) => {
                let mut names = HashSet::new();
                let mut duplicates = Vec::new();

                for provider in &providers {
                    if !names.insert(provider.name.clone()) {
                        duplicates.push(provider.name.clone());
                    }
                }

                if !duplicates.is_empty() {
                    report.add_error(format!(
                        "Duplicate provider names detected: {:?}\n\n\
                        Provider names must be unique for migration to work correctly.\n\
                        Please rename or remove duplicate providers before migrating.",
                        duplicates
                    ));
                }

                if providers.is_empty() {
                    report.add_warning("No providers found in source database".to_string());
                }
            }
            Err(e) => {
                report.add_error(format!("Failed to list source providers: {}", e));
            }
        }
    }

    async fn validate_no_orphaned_tokens(
        source_manager: &StorageManager, report: &mut PreMigrationValidationReport,
    ) {
        let providers = match source_manager.config_backend().list_providers().await {
            Ok(p) => p,
            Err(e) => {
                report.add_error(format!("Failed to list providers: {}", e));
                return;
            }
        };

        let tokens = match source_manager.token_store().await.get_all_tokens().await {
            Ok(t) => t,
            Err(e) => {
                report.add_error(format!("Failed to list tokens: {}", e));
                return;
            }
        };

        let provider_ids: HashSet<i64> = providers.iter().filter_map(|p| p.id).collect();
        let orphaned: Vec<i64> = tokens
            .keys()
            .filter(|id| !provider_ids.contains(id))
            .copied()
            .collect();

        if !orphaned.is_empty() {
            report.add_error(format!(
                "Found {} orphaned token(s) without corresponding providers: {:?}\n\n\
                This indicates data corruption or inconsistency.\n\
                Please investigate and remove orphaned tokens before migrating:\n\
                - Provider IDs with tokens: {:?}\n\
                - Valid provider IDs: {:?}",
                orphaned.len(),
                orphaned,
                tokens.keys().collect::<Vec<_>>(),
                provider_ids
            ));
        }
    }

    async fn validate_target_state(
        target_manager: &StorageManager, options: &MigrationOptions,
        report: &mut PreMigrationValidationReport,
    ) {
        let providers = match target_manager.config_backend().list_providers().await {
            Ok(p) => p,
            Err(e) => {
                tracing::debug!("Target database not accessible (may not exist yet): {}", e);
                return;
            }
        };

        let tokens = match target_manager.token_store().await.get_all_tokens().await {
            Ok(t) => t,
            Err(e) => {
                tracing::debug!("Target tokens not accessible: {}", e);
                HashMap::new()
            }
        };

        if !providers.is_empty() || !tokens.is_empty() {
            if !options.allow_non_empty_target {
                report.add_error(format!(
                    "Target database is not empty:\n\
                    - {} providers\n\
                    - {} tokens\n\n\
                    To prevent accidental data loss, migration requires an empty target database.\n\n\
                    Options:\n\
                    1. Use a different target database\n\
                    2. Clear the target database manually\n\
                    3. Set allow_non_empty_target=true to merge data (advanced)",
                    providers.len(),
                    tokens.len()
                ));
            } else {
                report.add_warning(format!(
                    "Target database is not empty ({} providers, {} tokens).\n\
                    Migration will attempt to merge data.\n\
                    Conflicts may occur if provider names overlap.",
                    providers.len(),
                    tokens.len()
                ));
            }
        }
    }

    async fn validate_postgres_schema(
        _target_config: &PipedashConfig, report: &mut PreMigrationValidationReport,
    ) {
        report.add_warning(
            "PostgreSQL schema validation not yet implemented.\n\
            Ensure you've run all database migrations before migrating data."
                .to_string(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_report() {
        let mut report = PreMigrationValidationReport::new();
        assert!(report.passed);
        assert!(report.errors.is_empty());

        report.add_warning("Test warning".to_string());
        assert!(report.passed);
        assert_eq!(report.warnings.len(), 1);

        report.add_error("Test error".to_string());
        assert!(!report.passed);
        assert_eq!(report.errors.len(), 1);
    }

    #[test]
    fn test_validation_report_into_result() {
        let mut report = PreMigrationValidationReport::new();
        assert!(report.clone().into_result().is_ok());

        report.add_error("Test error".to_string());
        assert!(report.into_result().is_err());
    }
}
