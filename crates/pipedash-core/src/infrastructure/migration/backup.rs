use std::path::{
    Path,
    PathBuf,
};

use chrono::Utc;
use serde::{
    Deserialize,
    Serialize,
};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{
    Connection,
    SqliteConnection,
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

    async fn backup_sqlite_database(src: &Path, dst: &Path) -> DomainResult<()> {
        let destination = dst.to_str().ok_or_else(|| {
            DomainError::DatabaseError(format!("Backup path is not valid UTF-8: {}", dst.display()))
        })?;
        let mut connection = SqliteConnection::connect_with(
            &SqliteConnectOptions::new().filename(src).read_only(true),
        )
        .await
        .map_err(|e| {
            DomainError::DatabaseError(format!("Failed to open SQLite database for backup: {}", e))
        })?;

        sqlx::query("VACUUM INTO ?")
            .bind(destination)
            .execute(&mut connection)
            .await
            .map_err(|e| {
                DomainError::DatabaseError(format!("Failed to backup SQLite database: {}", e))
            })?;

        connection.close().await.map_err(|e| {
            DomainError::DatabaseError(format!("Failed to close SQLite backup connection: {}", e))
        })
    }

    async fn restore_sqlite_database(backup: &Path, dst: &Path) -> DomainResult<()> {
        let source = backup.to_str().ok_or_else(|| {
            DomainError::DatabaseError(format!(
                "Backup path is not valid UTF-8: {}",
                backup.display()
            ))
        })?;
        let restore_error = |e: sqlx::Error| {
            DomainError::DatabaseError(format!("Failed to restore SQLite database: {}", e))
        };
        let mut connection = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(dst)
                .foreign_keys(false),
        )
        .await
        .map_err(restore_error)?;

        sqlx::query("ATTACH DATABASE ? AS backup")
            .bind(source)
            .execute(&mut connection)
            .await
            .map_err(restore_error)?;
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM backup.sqlite_master WHERE type = 'table' \
             AND (name NOT LIKE 'sqlite\\_%' ESCAPE '\\' OR name = 'sqlite_sequence')",
        )
        .fetch_all(&mut connection)
        .await
        .map_err(restore_error)?;

        let mut transaction = connection.begin().await.map_err(restore_error)?;
        for table in &tables {
            let table = table.replace('"', "\"\"");
            let sql = format!(
                "DELETE FROM main.\"{0}\"; INSERT INTO main.\"{0}\" SELECT * FROM backup.\"{0}\"",
                table
            );
            sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
                .execute(&mut *transaction)
                .await
                .map_err(restore_error)?;
        }
        transaction.commit().await.map_err(restore_error)?;

        sqlx::query("DETACH DATABASE backup")
            .execute(&mut connection)
            .await
            .map_err(restore_error)?;
        connection.close().await.map_err(restore_error)
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
                Self::backup_sqlite_database(&src, &dst).await?;

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
                        Self::backup_sqlite_database(&target_path, &backup_path).await?;

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
                if source_db.extension().is_some_and(|ext| ext == "json") {
                    tracing::warn!(
                        "PostgreSQL source backup exists but cannot be automatically restored"
                    );
                } else {
                    let original_path = Self::original_db_path(source_db)?;
                    Self::restore_sqlite_database(source_db, &original_path).await?;
                    tracing::info!("Source database restored from: {:?}", source_db);
                }
            }
        }

        if let Some(target_db) = &backups.target_db {
            if target_db.exists() {
                let original_path = Self::original_db_path(target_db)?;
                Self::restore_sqlite_database(target_db, &original_path).await?;
                tracing::info!("Target database restored from: {:?}", target_db);
            }
        }

        tracing::info!("Backup restoration complete");
        Ok(())
    }

    fn original_db_path(backup: &Path) -> DomainResult<PathBuf> {
        backup
            .parent()
            .map(|dir| dir.join("pipedash.db"))
            .ok_or_else(|| {
                DomainError::DatabaseError(format!("Backup path has no parent: {backup:?}"))
            })
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

        const RETENTION_DAYS: u64 = 7;
        let cutoff = SystemTime::now() - Duration::from_secs(RETENTION_DAYS * 24 * 3600);

        let Ok(mut entries) = tokio::fs::read_dir(&self.data_dir).await else {
            return Ok(());
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_backup =
                name.contains(".backup-") || (name.contains("_backup_") && name.ends_with(".json"));
            if !is_backup {
                continue;
            }
            let Ok(metadata) = entry.metadata().await else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            if modified >= cutoff {
                continue;
            }
            tokio::fs::remove_file(entry.path()).await.ok();
            tracing::debug!("Deleted old backup: {:?}", entry.path());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests;
