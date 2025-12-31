use std::collections::HashMap;
use std::sync::Arc;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::RwLock;
use tracing::{
    debug,
    info,
};

use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::storage::{
    ObjectMetadata,
    StorageBackend,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncDirection {
    Push,
    Pull,
    Bidirectional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictResolution {
    #[default]
    LastWriteWins,
    PreferLocal,
    PreferRemote,
    Skip,
}

#[derive(Debug, Clone)]
pub struct SyncConfig {
    pub direction: SyncDirection,
    pub conflict_resolution: ConflictResolution,
    pub include_prefixes: Vec<String>,
    pub exclude_prefixes: Vec<String>,
    pub delete_orphaned: bool,
    pub dry_run: bool,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            direction: SyncDirection::Bidirectional,
            conflict_resolution: ConflictResolution::LastWriteWins,
            include_prefixes: vec![],
            exclude_prefixes: vec![
                "pipedash.db".to_string(),
                "metrics.db".to_string(),
                "cache/".to_string(),
            ],
            delete_orphaned: false,
            dry_run: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncFileResult {
    pub key: String,
    pub action: SyncAction,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SyncAction {
    Pushed,
    Pulled,
    Skipped,
    DeletedRemote,
    DeletedLocal,
    NoChange,
    Conflict,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncResult {
    pub pushed: usize,
    pub pulled: usize,
    pub skipped: usize,
    pub conflicts: usize,
    pub deleted: usize,
    pub errors: usize,
    pub files: Vec<SyncFileResult>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncState {
    pub last_sync: Option<DateTime<Utc>>,
    pub file_checksums: HashMap<String, String>,
    pub file_timestamps: HashMap<String, DateTime<Utc>>,
}

pub struct SyncManager {
    local: Arc<dyn StorageBackend>,
    remote: Option<Arc<dyn StorageBackend>>,
    state: RwLock<SyncState>,
    default_config: SyncConfig,
}

impl SyncManager {
    pub fn new(local: Arc<dyn StorageBackend>) -> Self {
        Self {
            local,
            remote: None,
            state: RwLock::new(SyncState::default()),
            default_config: SyncConfig::default(),
        }
    }

    pub fn with_remote(local: Arc<dyn StorageBackend>, remote: Arc<dyn StorageBackend>) -> Self {
        Self {
            local,
            remote: Some(remote),
            state: RwLock::new(SyncState::default()),
            default_config: SyncConfig::default(),
        }
    }

    pub fn set_remote(&mut self, remote: Arc<dyn StorageBackend>) {
        self.remote = Some(remote);
    }

    pub fn clear_remote(&mut self) {
        self.remote = None;
    }

    pub fn has_remote(&self) -> bool {
        self.remote.is_some()
    }

    pub fn remote_type(&self) -> Option<&str> {
        self.remote.as_ref().map(|r| r.backend_type())
    }

    pub fn set_default_config(&mut self, config: SyncConfig) {
        self.default_config = config;
    }

    fn should_sync_key(&self, key: &str, config: &SyncConfig) -> bool {
        for prefix in &config.exclude_prefixes {
            if key.starts_with(prefix) {
                return false;
            }
        }

        if config.include_prefixes.is_empty() {
            return true;
        }

        for prefix in &config.include_prefixes {
            if key.starts_with(prefix) {
                return true;
            }
        }

        false
    }

    fn determine_action(
        &self, _key: &str, local_meta: Option<&ObjectMetadata>,
        remote_meta: Option<&ObjectMetadata>, config: &SyncConfig,
    ) -> SyncAction {
        match (local_meta, remote_meta) {
            (None, None) => SyncAction::NoChange,
            (Some(_), None) => match config.direction {
                SyncDirection::Push | SyncDirection::Bidirectional => SyncAction::Pushed,
                SyncDirection::Pull => {
                    if config.delete_orphaned {
                        SyncAction::DeletedLocal
                    } else {
                        SyncAction::Skipped
                    }
                }
            },
            (None, Some(_)) => match config.direction {
                SyncDirection::Pull | SyncDirection::Bidirectional => SyncAction::Pulled,
                SyncDirection::Push => {
                    if config.delete_orphaned {
                        SyncAction::DeletedRemote
                    } else {
                        SyncAction::Skipped
                    }
                }
            },
            (Some(local), Some(remote)) => {
                let local_id = local.etag.as_ref().unwrap_or(&"".to_string()).clone();
                let remote_id = remote.etag.as_ref().unwrap_or(&"".to_string()).clone();

                if !local_id.is_empty() && !remote_id.is_empty() && local_id == remote_id {
                    return SyncAction::NoChange;
                }

                match config.conflict_resolution {
                    ConflictResolution::LastWriteWins => {
                        if local.last_modified > remote.last_modified {
                            SyncAction::Pushed
                        } else if remote.last_modified > local.last_modified {
                            SyncAction::Pulled
                        } else {
                            SyncAction::NoChange
                        }
                    }
                    ConflictResolution::PreferLocal => SyncAction::Pushed,
                    ConflictResolution::PreferRemote => SyncAction::Pulled,
                    ConflictResolution::Skip => SyncAction::Conflict,
                }
            }
        }
    }

    pub async fn sync(&self, config: Option<SyncConfig>) -> DomainResult<SyncResult> {
        let config = config.unwrap_or_else(|| self.default_config.clone());
        let mut result = SyncResult {
            started_at: Utc::now(),
            ..Default::default()
        };

        let remote = match &self.remote {
            Some(r) => r,
            None => {
                info!("No remote storage configured, skipping sync");
                result.completed_at = Some(Utc::now());
                return Ok(result);
            }
        };

        if !remote.is_available().await {
            return Err(DomainError::NetworkError(
                "Remote storage is not available".to_string(),
            ));
        }

        info!(
            "Starting sync: direction={:?}, conflict_resolution={:?}",
            config.direction, config.conflict_resolution
        );

        let local_files = self.local.list(None).await?;
        let remote_files = remote.list(None).await?;

        let local_map: HashMap<String, ObjectMetadata> = local_files
            .into_iter()
            .filter(|f| self.should_sync_key(&f.key, &config))
            .map(|f| (f.key.clone(), f))
            .collect();

        let remote_map: HashMap<String, ObjectMetadata> = remote_files
            .into_iter()
            .filter(|f| self.should_sync_key(&f.key, &config))
            .map(|f| (f.key.clone(), f))
            .collect();

        let mut all_keys: Vec<String> = local_map.keys().cloned().collect();
        for key in remote_map.keys() {
            if !all_keys.contains(key) {
                all_keys.push(key.clone());
            }
        }

        for key in all_keys {
            let local_meta = local_map.get(&key);
            let remote_meta = remote_map.get(&key);
            let action = self.determine_action(&key, local_meta, remote_meta, &config);

            debug!("Processing {}: action={:?}", key, action);

            if config.dry_run {
                result.files.push(SyncFileResult {
                    key: key.clone(),
                    action,
                    success: true,
                    error: None,
                });
                match action {
                    SyncAction::Pushed => result.pushed += 1,
                    SyncAction::Pulled => result.pulled += 1,
                    SyncAction::Skipped | SyncAction::NoChange => result.skipped += 1,
                    SyncAction::Conflict => result.conflicts += 1,
                    SyncAction::DeletedLocal | SyncAction::DeletedRemote => result.deleted += 1,
                }
                continue;
            }

            let file_result = match action {
                SyncAction::Pushed => self.push_file(&key, remote.as_ref()).await,
                SyncAction::Pulled => self.pull_file(&key, remote.as_ref()).await,
                SyncAction::DeletedRemote => self.delete_remote(&key, remote.as_ref()).await,
                SyncAction::DeletedLocal => self.delete_local(&key).await,
                SyncAction::NoChange | SyncAction::Skipped | SyncAction::Conflict => {
                    Ok(SyncFileResult {
                        key: key.clone(),
                        action,
                        success: true,
                        error: None,
                    })
                }
            };

            match file_result {
                Ok(file_res) => {
                    match file_res.action {
                        SyncAction::Pushed => result.pushed += 1,
                        SyncAction::Pulled => result.pulled += 1,
                        SyncAction::Skipped | SyncAction::NoChange => result.skipped += 1,
                        SyncAction::Conflict => result.conflicts += 1,
                        SyncAction::DeletedLocal | SyncAction::DeletedRemote => result.deleted += 1,
                    }
                    result.files.push(file_res);
                }
                Err(e) => {
                    result.errors += 1;
                    result.files.push(SyncFileResult {
                        key: key.clone(),
                        action,
                        success: false,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        {
            let mut state = self.state.write().await;
            state.last_sync = Some(Utc::now());
        }

        result.completed_at = Some(Utc::now());
        info!(
            "Sync completed: pushed={}, pulled={}, skipped={}, conflicts={}, errors={}",
            result.pushed, result.pulled, result.skipped, result.conflicts, result.errors
        );

        Ok(result)
    }

    async fn push_file(
        &self, key: &str, remote: &dyn StorageBackend,
    ) -> DomainResult<SyncFileResult> {
        debug!("Pushing file: {}", key);
        let data = self.local.get(key).await?;
        remote.put(key, &data, None).await?;

        Ok(SyncFileResult {
            key: key.to_string(),
            action: SyncAction::Pushed,
            success: true,
            error: None,
        })
    }

    async fn pull_file(
        &self, key: &str, remote: &dyn StorageBackend,
    ) -> DomainResult<SyncFileResult> {
        debug!("Pulling file: {}", key);
        let data = remote.get(key).await?;
        self.local.put(key, &data, None).await?;

        Ok(SyncFileResult {
            key: key.to_string(),
            action: SyncAction::Pulled,
            success: true,
            error: None,
        })
    }

    async fn delete_remote(
        &self, key: &str, remote: &dyn StorageBackend,
    ) -> DomainResult<SyncFileResult> {
        debug!("Deleting remote file: {}", key);
        remote.delete(key).await?;

        Ok(SyncFileResult {
            key: key.to_string(),
            action: SyncAction::DeletedRemote,
            success: true,
            error: None,
        })
    }

    async fn delete_local(&self, key: &str) -> DomainResult<SyncFileResult> {
        debug!("Deleting local file: {}", key);
        self.local.delete(key).await?;

        Ok(SyncFileResult {
            key: key.to_string(),
            action: SyncAction::DeletedLocal,
            success: true,
            error: None,
        })
    }

    pub async fn force_push(&self) -> DomainResult<SyncResult> {
        self.sync(Some(SyncConfig {
            direction: SyncDirection::Push,
            conflict_resolution: ConflictResolution::PreferLocal,
            delete_orphaned: true,
            ..self.default_config.clone()
        }))
        .await
    }

    pub async fn force_pull(&self) -> DomainResult<SyncResult> {
        self.sync(Some(SyncConfig {
            direction: SyncDirection::Pull,
            conflict_resolution: ConflictResolution::PreferRemote,
            delete_orphaned: true,
            ..self.default_config.clone()
        }))
        .await
    }

    pub async fn preview(&self, config: Option<SyncConfig>) -> DomainResult<SyncResult> {
        let mut config = config.unwrap_or_else(|| self.default_config.clone());
        config.dry_run = true;
        self.sync(Some(config)).await
    }

    pub async fn last_sync_time(&self) -> Option<DateTime<Utc>> {
        self.state.read().await.last_sync
    }

    pub async fn get_state(&self) -> SyncState {
        self.state.read().await.clone()
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::infrastructure::storage::LocalStorage;

    #[tokio::test]
    async fn test_sync_manager_creation() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(LocalStorage::new(temp_dir.path().to_path_buf()));
        let manager = SyncManager::new(storage);

        assert!(!manager.has_remote());
        assert!(manager.remote_type().is_none());
    }

    #[tokio::test]
    async fn test_sync_without_remote() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(LocalStorage::new(temp_dir.path().to_path_buf()));
        let manager = SyncManager::new(storage);

        let result = manager.sync(None).await.unwrap();
        assert_eq!(result.pushed, 0);
        assert_eq!(result.pulled, 0);
        assert_eq!(result.errors, 0);
    }

    #[tokio::test]
    async fn test_should_sync_key() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(LocalStorage::new(temp_dir.path().to_path_buf()));
        let manager = SyncManager::new(storage);

        let config = SyncConfig {
            exclude_prefixes: vec!["cache/".to_string(), ".git/".to_string()],
            include_prefixes: vec![],
            ..Default::default()
        };

        assert!(manager.should_sync_key("config/settings.json", &config));
        assert!(!manager.should_sync_key("cache/temp.bin", &config));
        assert!(!manager.should_sync_key(".git/config", &config));
    }

    #[tokio::test]
    async fn test_sync_between_local_storages() {
        let local_dir = TempDir::new().unwrap();
        let remote_dir = TempDir::new().unwrap();

        let local = Arc::new(LocalStorage::new(local_dir.path().to_path_buf()));
        let remote: Arc<dyn StorageBackend> =
            Arc::new(LocalStorage::new(remote_dir.path().to_path_buf()));

        local.put("test.txt", b"hello world", None).await.unwrap();

        let manager = SyncManager::with_remote(local.clone(), remote.clone());

        let result = manager
            .sync(Some(SyncConfig {
                direction: SyncDirection::Push,
                ..Default::default()
            }))
            .await
            .unwrap();

        assert_eq!(result.pushed, 1);
        assert_eq!(result.errors, 0);

        let remote_data = remote.get("test.txt").await.unwrap();
        assert_eq!(remote_data, b"hello world");
    }
}
