use std::time::Instant;

use super::backup::{
    BackupManager,
    BackupPaths,
};
use super::{
    MigrationOptions,
    MigrationOrchestrator,
    MigrationPlan,
    MigrationResult,
    MigrationStats,
};
use crate::domain::DomainError;
use crate::infrastructure::config::StorageManager;

#[cfg(test)]
mod tests;

fn source_db_display(backups: &BackupPaths) -> String {
    backups
        .source_db
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "N/A".to_string())
}

fn recovery_message(
    error: &DomainError, backups: &BackupPaths, restored: Result<(), DomainError>,
) -> String {
    match restored {
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
                source_db_display(backups),
                error
            )
        }
        Err(restore_err) => format!(
            "CRITICAL ERROR: Migration failed AND backup restoration failed!\n\n\
             Original error: {}\n\
             Restore error: {}\n\n\
             MANUAL RECOVERY REQUIRED:\n\
             1. Restore config file from: {}\n\
             2. Restore database from: {}\n\
             3. Review logs for details\n\
             4. Contact support if needed",
            error,
            restore_err,
            backups.source_config.display(),
            source_db_display(backups)
        ),
    }
}

impl MigrationOrchestrator {
    async fn open_target(
        &self, plan: &MigrationPlan, is_desktop: bool,
    ) -> Result<StorageManager, (&'static str, String)> {
        match &self.target_token_store {
            Some(token_store) => {
                StorageManager::with_token_store(plan.to.clone(), token_store.clone(), is_desktop)
                    .await
                    .map_err(|e| {
                        (
                            "Failed to create target manager with token store",
                            e.to_string(),
                        )
                    })
            }
            None => StorageManager::from_config(plan.to.clone(), false)
                .await
                .map_err(|e| ("Failed to create target manager", e.to_string())),
        }
    }

    pub async fn execute_migration(
        &self, plan: MigrationPlan, options: MigrationOptions, is_desktop: bool,
    ) -> MigrationResult {
        let start_time = Instant::now();
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

        let plan = match self.plan_migration(plan.to, &options) {
            Ok(plan) => plan,
            Err(error) => {
                result.errors.push(error.to_string());
                result.duration_ms = start_time.elapsed().as_millis() as u64;
                return result;
            }
        };

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

        let target_manager = match self.open_target(&plan, is_desktop).await {
            Ok(manager) => manager,
            Err((what, error)) => {
                result.errors.push(format!(
                    "{what}: {error}\n\n\
                    INFO: Migration aborted before any changes were made.\n\
                    INFO: Your data is safe - no modifications were performed."
                ));
                backup_manager.cleanup_backups(&backups, false).await.ok();
                result.duration_ms = start_time.elapsed().as_millis() as u64;
                return result;
            }
        };

        for (idx, step) in plan.steps.iter().enumerate() {
            self.emit_progress(*step, idx, plan.steps.len()).await;

            let step_result = self
                .execute_step(*step, &plan, &options, &target_manager, &mut result)
                .await;

            if let Err(e) = step_result {
                let step_name = format!("{:?}", step);

                tracing::error!(
                    "Migration failed at step {} - restoring from backups",
                    step_name
                );

                let restored = backup_manager.restore_backups(&backups).await;
                let recovery_instructions = recovery_message(&e, &backups, restored);

                result.errors.push(format!(
                    "Migration failed at step '{}':\n{}\n\n{}",
                    step_name, e, recovery_instructions
                ));

                self.rollback(&result).await;
                result.duration_ms = start_time.elapsed().as_millis() as u64;
                return result;
            }
            result.steps_completed.push(*step);
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
}
