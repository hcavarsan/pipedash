use std::path::PathBuf;

use chrono::Utc;
use serde::{
    Deserialize,
    Serialize,
};

use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::config::{
    PipedashConfig,
    StorageBackend,
};
use crate::infrastructure::ConfigBackend;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupPaths {
    pub source_db: Option<PathBuf>,
    pub source_config: PathBuf,
    pub target_db: Option<PathBuf>,
    pub timestamp: String,
}

pub struct BackupManager {
    data_dir: PathBuf,
}

impl BackupManager {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }

    pub async fn create_backups(
        &self, source_config: &PipedashConfig, target_config: &PipedashConfig,
        source_backend: Option<&dyn ConfigBackend>,
    ) -> DomainResult<BackupPaths> {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();

        tracing::info!("Creating migration backups with timestamp: {}", timestamp);

        let source_db_backup = self
            .backup_source_database(source_config, source_backend, &timestamp)
            .await?;

        let config_backup = self.backup_config_file(&timestamp).await?;

        let target_db_backup = self
            .backup_target_database(target_config, &timestamp)
            .await?;

        let backups = BackupPaths {
            source_db: source_db_backup,
            source_config: config_backup,
            target_db: target_db_backup,
            timestamp,
        };

        tracing::info!("Backups created successfully: {:?}", backups);

        Ok(backups)
    }

    async fn backup_source_database(
        &self, source_config: &PipedashConfig, source_backend: Option<&dyn ConfigBackend>,
        timestamp: &str,
    ) -> DomainResult<Option<PathBuf>> {
        match source_config.storage.backend {
            StorageBackend::Sqlite => {
                let src = source_config.db_path();
                if !src.exists() {
                    tracing::warn!("Source database does not exist: {:?}", src);
                    return Ok(None);
                }

                let dst = src.with_file_name(format!("pipedash.db.backup-{}", timestamp));
                tokio::fs::copy(&src, &dst).await.map_err(|e| {
                    DomainError::DatabaseError(format!("Failed to backup SQLite database: {}", e))
                })?;

                tracing::info!("SQLite database backed up to: {:?}", dst);
                Ok(Some(dst))
            }
            StorageBackend::Postgres => {
                if let Some(backend) = source_backend {
                    let export = backend.export_all().await?;
                    let backup_path = self
                        .data_dir
                        .join(format!("postgres_backup_{}.json", timestamp));

                    let json = serde_json::to_string_pretty(&export).map_err(|e| {
                        DomainError::DatabaseError(format!(
                            "Failed to serialize PostgreSQL backup: {}",
                            e
                        ))
                    })?;

                    tokio::fs::write(&backup_path, json).await.map_err(|e| {
                        DomainError::DatabaseError(format!(
                            "Failed to write PostgreSQL backup: {}",
                            e
                        ))
                    })?;

                    tracing::info!("PostgreSQL database exported to: {:?}", backup_path);
                    Ok(Some(backup_path))
                } else {
                    tracing::warn!("No source backend provided for PostgreSQL backup");
                    Ok(None)
                }
            }
        }
    }

    async fn backup_config_file(&self, timestamp: &str) -> DomainResult<PathBuf> {
        use crate::infrastructure::config::ConfigLoader;

        let config_path = ConfigLoader::discover_config_path();
        if !config_path.exists() {
            return Err(DomainError::InvalidConfig(format!(
                "Config file not found: {:?}",
                config_path
            )));
        }

        let backup_path = config_path.with_file_name(format!("config.toml.backup-{}", timestamp));
        tokio::fs::copy(&config_path, &backup_path)
            .await
            .map_err(|e| {
                DomainError::DatabaseError(format!("Failed to backup config file: {}", e))
            })?;

        tracing::info!("Config file backed up to: {:?}", backup_path);
        Ok(backup_path)
    }

    async fn backup_target_database(
        &self, target_config: &PipedashConfig, timestamp: &str,
    ) -> DomainResult<Option<PathBuf>> {
        match target_config.storage.backend {
            StorageBackend::Sqlite => {
                let target_path = target_config.db_path();
                if !target_path.exists() {
                    return Ok(None);
                }

                let metadata = tokio::fs::metadata(&target_path).await.ok();
                if let Some(meta) = metadata {
                    if meta.len() > 0 {
                        let backup_path = target_path
                            .with_file_name(format!("pipedash_target.db.backup-{}", timestamp));
                        tokio::fs::copy(&target_path, &backup_path)
                            .await
                            .map_err(|e| {
                                DomainError::DatabaseError(format!(
                                    "Failed to backup target database: {}",
                                    e
                                ))
                            })?;

                        tracing::info!("Target database backed up to: {:?}", backup_path);
                        return Ok(Some(backup_path));
                    }
                }
                Ok(None)
            }
            StorageBackend::Postgres => Ok(None),
        }
    }

    pub async fn restore_backups(&self, backups: &BackupPaths) -> DomainResult<()> {
        tracing::warn!("Migration failed - restoring from backups");

        use crate::infrastructure::config::ConfigLoader;
        let config_path = ConfigLoader::discover_config_path();
        if backups.source_config.exists() {
            tokio::fs::copy(&backups.source_config, &config_path)
                .await
                .map_err(|e| {
                    DomainError::DatabaseError(format!("Failed to restore config file: {}", e))
                })?;
            tracing::info!("Config file restored from: {:?}", backups.source_config);
        }

        if let Some(source_db) = &backups.source_db {
            if source_db.exists() {
                let original_path = if source_db.ends_with(".json") {
                    tracing::warn!(
                        "PostgreSQL source backup exists but cannot be automatically restored"
                    );
                    return Ok(());
                } else {
                    source_db.parent().unwrap().join("pipedash.db")
                };

                tokio::fs::copy(source_db, &original_path)
                    .await
                    .map_err(|e| {
                        DomainError::DatabaseError(format!(
                            "Failed to restore source database: {}",
                            e
                        ))
                    })?;
                tracing::info!("Source database restored from: {:?}", source_db);
            }
        }

        tracing::info!("Backup restoration complete");
        Ok(())
    }

    pub async fn cleanup_backups(&self, backups: &BackupPaths, keep: bool) -> DomainResult<()> {
        if keep {
            tracing::info!("Keeping backups as requested");
            return Ok(());
        }

        tracing::info!("Cleaning up migration backups");

        if let Some(source_db) = &backups.source_db {
            if source_db.exists() {
                tokio::fs::remove_file(source_db).await.ok();
                tracing::debug!("Deleted source database backup: {:?}", source_db);
            }
        }

        if backups.source_config.exists() {
            tokio::fs::remove_file(&backups.source_config).await.ok();
            tracing::debug!("Deleted config backup: {:?}", backups.source_config);
        }

        if let Some(target_db) = &backups.target_db {
            if target_db.exists() {
                tokio::fs::remove_file(target_db).await.ok();
                tracing::debug!("Deleted target database backup: {:?}", target_db);
            }
        }

        self.cleanup_old_backups().await?;

        Ok(())
    }

    async fn cleanup_old_backups(&self) -> DomainResult<()> {
        use std::time::{
            Duration,
            SystemTime,
        };

        let retention_days = 7;
        let cutoff = SystemTime::now() - Duration::from_secs(retention_days * 24 * 3600);

        let patterns = vec!["*.backup-*", "*_backup_*.json"];

        for pattern in patterns {
            let glob_pattern = self.data_dir.join(pattern);
            if let Ok(entries) = glob::glob(glob_pattern.to_str().unwrap()) {
                for entry in entries.flatten() {
                    if let Ok(metadata) = tokio::fs::metadata(&entry).await {
                        if let Ok(modified) = metadata.modified() {
                            if modified < cutoff {
                                tokio::fs::remove_file(&entry).await.ok();
                                tracing::debug!("Deleted old backup: {:?}", entry);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub async fn estimate_backup_size(&self, config: &PipedashConfig) -> u64 {
        let mut total_size = 0u64;

        let db_path = config.db_path();
        if let Ok(metadata) = tokio::fs::metadata(&db_path).await {
            total_size += metadata.len();
        }

        use crate::infrastructure::config::ConfigLoader;
        let config_path = ConfigLoader::discover_config_path();
        if let Ok(metadata) = tokio::fs::metadata(&config_path).await {
            total_size += metadata.len();
        }

        total_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires filesystem access
    async fn test_backup_manager_creation() {
        let temp_dir = std::env::temp_dir().join("pipedash_backup_test");
        let manager = BackupManager::new(temp_dir);
        assert!(manager.data_dir.ends_with("pipedash_backup_test"));
    }
}
