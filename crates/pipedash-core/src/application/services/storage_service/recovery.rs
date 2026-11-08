use std::future::Future;
use std::path::{
    Path,
    PathBuf,
};
use std::time::Duration;

use super::blocking;
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::{
    PipedashConfig,
    StorageManager,
};

#[derive(Debug, thiserror::Error)]
pub enum RecoveryFailure {
    #[error("{0}")]
    Build(DomainError),
    #[error("{0}")]
    Recovery(String),
}

#[derive(Default)]
struct BackupInfo {
    main_db_backup: Option<PathBuf>,
    main_db_original: Option<PathBuf>,
    metrics_db_backup: Option<PathBuf>,
    metrics_db_original: Option<PathBuf>,
}

const LOCK_RETRIES: u32 = 40;
const LOCK_RETRY_DELAY: Duration = Duration::from_millis(50);

fn is_locked(error: &std::io::Error) -> bool {
    cfg!(windows) && matches!(error.raw_os_error(), Some(32 | 33))
}

fn retry_while_locked(mut op: impl FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
    for _ in 0..LOCK_RETRIES {
        match op() {
            Err(error) if is_locked(&error) => std::thread::sleep(LOCK_RETRY_DELAY),
            result => return result,
        }
    }
    op()
}

pub async fn build_with_recovery<F, Fut>(
    config: &PipedashConfig, may_recover: bool, build: F,
) -> Result<StorageManager, RecoveryFailure>
where
    F: Fn() -> Fut,
    Fut: Future<Output = DomainResult<StorageManager>>,
{
    let error = match build().await {
        Ok(manager) => return Ok(manager),
        Err(e) => e,
    };

    if !may_recover || !is_database_corruption_error(&error.to_string()) {
        return Err(RecoveryFailure::Build(error));
    }

    let error_msg = error.to_string();
    tracing::error!(
        "Database corruption detected: {}. Attempting recovery...",
        error_msg
    );

    let data_dir = config.data_dir();
    let backup_info = reset(&data_dir).await.map_err(|reset_err| {
        tracing::error!("Database backup failed: {}", reset_err);
        RecoveryFailure::Recovery(format!(
            "Failed to backup corrupted databases: {}. \
            Original error: {}. \
            Please try deleting the data directory manually: {:?}",
            reset_err, error_msg, data_dir
        ))
    })?;

    tracing::info!("Databases backed up. Retrying initialization...");

    match build().await {
        Ok(manager) => {
            tracing::info!("Recovery successful - initialized with fresh databases");
            Ok(manager)
        }
        Err(retry_err) => {
            tracing::error!(
                "Recovery failed: {}. Rolling back database backups...",
                retry_err
            );

            if let Err(restore_err) = restore(backup_info).await {
                tracing::error!(
                    "Failed to restore database backups: {}. \
                    Backups are still available in corrupt_backups directory.",
                    restore_err
                );
            }

            Err(RecoveryFailure::Recovery(format!(
                "Recovery failed: {}. Databases have been restored to original state. \
                Please check your configuration or contact support.",
                retry_err
            )))
        }
    }
}

async fn reset(data_dir: &Path) -> Result<BackupInfo, String> {
    let data_dir = data_dir.to_path_buf();
    blocking(move || reset_corrupted_databases(&data_dir))
        .await
        .map_err(|e| e.to_string())?
}

async fn restore(backup_info: BackupInfo) -> Result<(), String> {
    blocking(move || restore_database_backups(&backup_info))
        .await
        .map_err(|e| e.to_string())?
}

fn is_database_corruption_error(error_msg: &str) -> bool {
    let msg = error_msg.to_lowercase();
    msg.contains("corrupt")
        || msg.contains("malformed")
        || msg.contains("database disk image")
        || msg.contains("file is not a database")
        || msg.contains("database is locked") && msg.contains("timeout")
        || msg.contains("unable to open database")
}

fn backup_database(
    original: &Path, backup_dir: &Path, prefix: &str, timestamp: impl std::fmt::Display,
    label: &str,
) -> Result<Option<PathBuf>, String> {
    if !original.exists() {
        return Ok(None);
    }

    let backup_name = format!("{}_{}.db.corrupt", prefix, timestamp);
    let backup_path = backup_dir.join(&backup_name);
    if let Err(e) = retry_while_locked(|| std::fs::rename(original, &backup_path)) {
        tracing::warn!("Could not backup {label} db: {}. Removing directly.", e);
        retry_while_locked(|| std::fs::remove_file(original))
            .map_err(|e| format!("Failed to remove corrupted {label} db: {}", e))?;
        return Ok(None);
    }

    tracing::info!("Backed up {label} db to: {}", backup_name);
    Ok(Some(backup_path))
}

fn reset_corrupted_databases(data_dir: &Path) -> Result<BackupInfo, String> {
    let db_path = data_dir.join("pipedash.db");
    let metrics_db_path = data_dir.join("metrics.db");

    let backup_dir = data_dir.join("corrupt_backups");
    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("Failed to create backup directory: {}", e))?;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let main_db_backup = backup_database(&db_path, &backup_dir, "pipedash", &timestamp, "main")?;
    let metrics_db_backup = backup_database(
        &metrics_db_path,
        &backup_dir,
        "metrics",
        &timestamp,
        "metrics",
    )?;

    Ok(BackupInfo {
        main_db_original: main_db_backup.as_ref().map(|_| db_path),
        main_db_backup,
        metrics_db_original: metrics_db_backup.as_ref().map(|_| metrics_db_path),
        metrics_db_backup,
    })
}

fn restore_backup(
    backup: &Option<PathBuf>, original: &Option<PathBuf>, label: &str,
) -> Result<bool, String> {
    let (Some(backup), Some(original)) = (backup, original) else {
        return Ok(false);
    };
    if !backup.exists() || original.exists() {
        return Ok(false);
    }

    retry_while_locked(|| std::fs::rename(backup, original))
        .map_err(|e| format!("Failed to restore {label} db from backup: {}", e))?;
    tracing::info!("Restored {label} database from backup");
    Ok(true)
}

fn restore_database_backups(backup_info: &BackupInfo) -> Result<(), String> {
    let main = restore_backup(
        &backup_info.main_db_backup,
        &backup_info.main_db_original,
        "main",
    )?;
    let metrics = restore_backup(
        &backup_info.metrics_db_backup,
        &backup_info.metrics_db_original,
        "metrics",
    )?;

    if main || metrics {
        tracing::info!("Databases restored to original state");
    }

    Ok(())
}

#[cfg(test)]
mod tests;
