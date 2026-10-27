use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;

use pipedash_core::domain::{
    DomainError,
    DomainResult,
};
use pipedash_core::event::CoreEvent;
use pipedash_core::infrastructure::config::{
    ConfigLoader,
    PipedashConfig,
};
use pipedash_core::infrastructure::database::{
    has_encrypted_tokens,
    init_database,
};
use pipedash_core::infrastructure::secrets::{
    MemoryTokenStore,
    SqliteTokenStore,
};
use pipedash_core::infrastructure::{
    StorageManager,
    TokenStore,
};
use pipedash_core::CoreContext;
use tauri::{
    AppHandle,
    Manager,
};

use crate::app_lock::AppLockState;
use crate::fallback_store::FallbackTokenStore;
use crate::keyring_store::KeyringTokenStore;
use crate::tauri_event_bus::create_tauri_event_bus;
use crate::{
    AppDataDir,
    MaybeCoreContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootKind {
    Launch,
    Setup,
    Unlock,
    Resume,
}

impl BootKind {
    fn recovers_corruption(self) -> bool {
        matches!(self, Self::Launch | Self::Resume)
    }

    fn announces_unlock(self) -> bool {
        matches!(self, Self::Unlock | Self::Resume)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageFacts {
    pub sqlite: bool,
    pub vault_password_set: bool,
    pub has_encrypted_tokens: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageChoice {
    Keyring,
    Locked,
    EncryptedWithKeyringFallback,
    FromConfig { desktop: bool, allow_locked: bool },
}

pub fn choose_storage(kind: BootKind, facts: StorageFacts) -> StorageChoice {
    match kind {
        BootKind::Launch | BootKind::Resume => {
            if facts.sqlite && !facts.vault_password_set && !facts.has_encrypted_tokens {
                StorageChoice::Keyring
            } else if !facts.vault_password_set {
                StorageChoice::Locked
            } else if facts.sqlite {
                StorageChoice::EncryptedWithKeyringFallback
            } else {
                StorageChoice::FromConfig {
                    desktop: false,
                    allow_locked: true,
                }
            }
        }
        BootKind::Setup if facts.sqlite => StorageChoice::Keyring,
        BootKind::Setup => StorageChoice::FromConfig {
            desktop: true,
            allow_locked: true,
        },
        BootKind::Unlock => StorageChoice::FromConfig {
            desktop: true,
            allow_locked: false,
        },
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BootError {
    #[error("Failed to load configuration: {0}")]
    Config(String),
    #[error("{0}")]
    Storage(String),
    #[error("Invalid password: {0}")]
    InvalidPassword(String),
    #[error("Invalid password: failed to decrypt tokens: {0}")]
    Decrypt(String),
    #[error("Failed to create CoreContext: {0}")]
    Context(String),
    #[error("Use Face ID to unlock Pipedash")]
    Locked,
}

fn boot_denied(app_lock_enabled: bool, locked: bool, authenticated: bool) -> bool {
    app_lock_enabled && locked && !authenticated
}

pub async fn boot_core(
    app: &AppHandle, kind: BootKind, authenticated: bool,
) -> Result<Arc<CoreContext>, BootError> {
    let lock = app.state::<AppLockState>();
    let _transition = lock.transition().await;

    if boot_denied(lock.settings().enabled, lock.locked(), authenticated) {
        return Err(BootError::Locked);
    }

    let app_data_dir = app.state::<AppDataDir>().0.clone();
    let config = load_config(&app_data_dir)?;

    let facts = StorageFacts {
        sqlite: config.storage.backend.is_sqlite(),
        vault_password_set: std::env::var("PIPEDASH_VAULT_PASSWORD").is_ok(),
        has_encrypted_tokens: has_encrypted_tokens(&config.data_dir().join("pipedash.db")).await,
    };
    let choice = choose_storage(kind, facts);
    log_choice(choice, facts);

    let storage_manager = create_storage_manager(kind, choice, &config, &app_data_dir).await?;

    if kind == BootKind::Unlock {
        match storage_manager.token_store().await.get_all_tokens().await {
            Ok(tokens) => tracing::info!(
                "Password validated - decrypted {} tokens successfully",
                tokens.len()
            ),
            Err(e) => return Err(BootError::Decrypt(e.to_string())),
        }
    }

    let event_bus = create_tauri_event_bus(app.clone());
    let core = CoreContext::with_storage_manager(&storage_manager, event_bus)
        .await
        .map(Arc::new)
        .map_err(|e| BootError::Context(e.to_string()))?;

    let maybe_core = app.state::<MaybeCoreContext>();
    let old = maybe_core.0.write().await.replace(Arc::clone(&core));
    if let Some(old) = old {
        crate::build_watch::interrupt(app);
        old.shutdown().await;
    }
    lock.set_locked(false);

    if choice == StorageChoice::Locked {
        tracing::info!(
            "Vault locked - background tasks deferred until unlock. \
             Providers will not be loaded until vault is unlocked."
        );
    } else {
        spawn_background_tasks(Arc::clone(&core), kind);
    }

    tracing::info!("Pipedash initialized with CoreContext ({:?})", kind);
    crate::local_notifications::schedule_sync(app);
    Ok(core)
}

fn load_config(app_data_dir: &Path) -> Result<PipedashConfig, BootError> {
    ConfigLoader::load(&app_data_dir.join("config.toml"))
        .map_err(|e| BootError::Config(e.to_string()))
}

pub fn launch_outcome<T>(result: Result<T, BootError>) -> Result<(), BootError> {
    match result {
        Ok(_) => Ok(()),
        Err(e @ BootError::Config(_)) => {
            tracing::error!("{} - starting without the core to show the config error", e);
            Ok(())
        }
        Err(e) => {
            tracing::error!("CoreContext initialization failed: {}", e);
            Err(e)
        }
    }
}

fn log_choice(choice: StorageChoice, facts: StorageFacts) {
    match choice {
        StorageChoice::Keyring => tracing::info!(
            "Using system keyring for credential storage (desktop SQLite default - no encrypted tokens found)"
        ),
        StorageChoice::Locked => tracing::info!(
            "Vault is locked: encrypted tokens found but no password provided. \
            Starting in locked mode - UI will prompt for unlock."
        ),
        StorageChoice::EncryptedWithKeyringFallback => tracing::info!(
            "Using encrypted database with keyring fallback for credential storage \
            (migrating tokens from keyring to encrypted storage)"
        ),
        StorageChoice::FromConfig { .. } if facts.sqlite => {
            tracing::info!("Using encrypted database for credential storage (vault password detected)")
        }
        StorageChoice::FromConfig { .. } => {
            tracing::info!("Using encrypted database for credential storage (PostgreSQL backend)")
        }
    }
}

fn spawn_background_tasks(core: Arc<CoreContext>, kind: BootKind) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = core.warmup_token_store().await {
            tracing::warn!("Token store warmup failed: {}", e);
        }

        if kind == BootKind::Unlock {
            if let Err(e) = core.provider_service.load_all_providers().await {
                tracing::warn!("Failed to load providers after unlock: {}", e);
            }
        }

        core.start_background_tasks().await;

        if kind.announces_unlock() {
            core.event_bus.emit(CoreEvent::VaultUnlocked).await;
        }
    });
}

async fn create_storage_manager(
    kind: BootKind, choice: StorageChoice, config: &PipedashConfig, app_data_dir: &Path,
) -> Result<StorageManager, BootError> {
    let error = match build_storage_manager(choice, config).await {
        Ok(manager) => return Ok(manager),
        Err(e) => e,
    };

    let error_msg = error.to_string();
    if !kind.recovers_corruption() || !is_database_corruption_error(&error_msg) {
        return Err(match kind {
            BootKind::Setup => {
                tracing::error!("StorageManager initialization failed: {}", error_msg);
                BootError::Storage(format!(
                    "Failed to initialize StorageManager: {}",
                    error_msg
                ))
            }
            BootKind::Unlock => {
                tracing::warn!("Vault unlock failed - invalid password: {}", error_msg);
                BootError::InvalidPassword(error_msg)
            }
            BootKind::Launch | BootKind::Resume => {
                tracing::error!(
                    "StorageManager initialization failed (not database corruption): {}",
                    error_msg
                );
                let hint = if cfg!(mobile) {
                    ""
                } else {
                    " Check your PIPEDASH_VAULT_PASSWORD environment variable."
                };
                BootError::Storage(format!(
                    "Failed to initialize storage: {}. \
                    This appears to be a configuration issue, not database corruption.{}",
                    error_msg, hint
                ))
            }
        });
    }

    tracing::error!(
        "Database corruption detected: {}. Attempting recovery...",
        error_msg
    );

    let backup_info = reset_corrupted_databases(app_data_dir).map_err(|reset_err| {
        tracing::error!("Database backup failed: {}", reset_err);
        BootError::Storage(format!(
            "Failed to backup corrupted databases: {}. \
            Original error: {}. \
            Please try deleting the app data directory manually: {:?}",
            reset_err, error_msg, app_data_dir
        ))
    })?;

    tracing::info!("Databases backed up. Retrying initialization...");

    match build_storage_manager(choice, config).await {
        Ok(manager) => {
            tracing::info!("Recovery successful - initialized with fresh databases");
            Ok(manager)
        }
        Err(retry_err) => {
            tracing::error!(
                "Recovery failed: {}. Rolling back database backups...",
                retry_err
            );

            if let Err(restore_err) = restore_database_backups(&backup_info) {
                tracing::error!(
                    "Failed to restore database backups: {}. \
                    Backups are still available in corrupt_backups directory.",
                    restore_err
                );
            }

            Err(BootError::Storage(format!(
                "Recovery failed: {}. Databases have been restored to original state. \
                Please check your configuration or contact support.",
                retry_err
            )))
        }
    }
}

async fn build_storage_manager(
    choice: StorageChoice, config: &PipedashConfig,
) -> DomainResult<StorageManager> {
    match choice {
        StorageChoice::Keyring => {
            let token_store: Arc<dyn TokenStore> = Arc::new(KeyringTokenStore::new());
            StorageManager::with_token_store(config.clone(), token_store, true).await
        }
        StorageChoice::Locked => {
            tracing::warn!(
                "Starting with locked vault - tokens will be inaccessible until unlocked"
            );
            let token_store: Arc<dyn TokenStore> = Arc::new(MemoryTokenStore::new());
            StorageManager::with_token_store_locked(config.clone(), token_store, true).await
        }
        StorageChoice::EncryptedWithKeyringFallback => {
            let db_path = config.data_dir().join("pipedash.db");
            let pool = init_database(db_path).await.map_err(|e| {
                DomainError::DatabaseError(format!("Failed to initialize SQLite database: {}", e))
            })?;

            let primary_store: Arc<dyn TokenStore> =
                Arc::new(SqliteTokenStore::new(pool, None).await?);
            let fallback_store: Arc<dyn TokenStore> = Arc::new(KeyringTokenStore::new());
            let token_store: Arc<dyn TokenStore> =
                Arc::new(FallbackTokenStore::new(primary_store, fallback_store));

            tracing::info!(
                "Created FallbackTokenStore: encrypted SQLite (primary) + keyring (fallback)"
            );

            StorageManager::with_token_store(config.clone(), token_store, true).await
        }
        StorageChoice::FromConfig {
            desktop,
            allow_locked: true,
        } => StorageManager::from_config_allow_locked(config.clone(), desktop).await,
        StorageChoice::FromConfig {
            desktop,
            allow_locked: false,
        } => StorageManager::from_config(config.clone(), desktop).await,
    }
}

#[derive(Default)]
struct BackupInfo {
    main_db_backup: Option<PathBuf>,
    main_db_original: Option<PathBuf>,
    metrics_db_backup: Option<PathBuf>,
    metrics_db_original: Option<PathBuf>,
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

fn reset_corrupted_databases(app_data_dir: &Path) -> Result<BackupInfo, String> {
    let db_path = app_data_dir.join("pipedash.db");
    let metrics_db_path = app_data_dir.join("metrics.db");

    let backup_dir = app_data_dir.join("corrupt_backups");
    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("Failed to create backup directory: {}", e))?;

    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let mut backup_info = BackupInfo::default();

    if db_path.exists() {
        let backup_name = format!("pipedash_{}.db.corrupt", timestamp);
        let backup_path = backup_dir.join(&backup_name);
        if let Err(e) = std::fs::rename(&db_path, &backup_path) {
            tracing::warn!("Could not backup main db: {}. Removing directly.", e);
            std::fs::remove_file(&db_path)
                .map_err(|e| format!("Failed to remove corrupted main db: {}", e))?;
        } else {
            tracing::info!("Backed up main db to: {}", backup_name);
            backup_info.main_db_backup = Some(backup_path);
            backup_info.main_db_original = Some(db_path.clone());
        }
    }

    if metrics_db_path.exists() {
        let backup_name = format!("metrics_{}.db.corrupt", timestamp);
        let backup_path = backup_dir.join(&backup_name);
        if let Err(e) = std::fs::rename(&metrics_db_path, &backup_path) {
            tracing::warn!("Could not backup metrics db: {}. Removing directly.", e);
            std::fs::remove_file(&metrics_db_path)
                .map_err(|e| format!("Failed to remove corrupted metrics db: {}", e))?;
        } else {
            tracing::info!("Backed up metrics db to: {}", backup_name);
            backup_info.metrics_db_backup = Some(backup_path);
            backup_info.metrics_db_original = Some(metrics_db_path.clone());
        }
    }

    Ok(backup_info)
}

fn restore_database_backups(backup_info: &BackupInfo) -> Result<(), String> {
    let mut restored = false;

    if let (Some(backup), Some(original)) =
        (&backup_info.main_db_backup, &backup_info.main_db_original)
    {
        if backup.exists() && !original.exists() {
            std::fs::rename(backup, original)
                .map_err(|e| format!("Failed to restore main db from backup: {}", e))?;
            tracing::info!("Restored main database from backup");
            restored = true;
        }
    }

    if let (Some(backup), Some(original)) = (
        &backup_info.metrics_db_backup,
        &backup_info.metrics_db_original,
    ) {
        if backup.exists() && !original.exists() {
            std::fs::rename(backup, original)
                .map_err(|e| format!("Failed to restore metrics db from backup: {}", e))?;
            tracing::info!("Restored metrics database from backup");
            restored = true;
        }
    }

    if restored {
        tracing::info!("Databases restored to original state");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAUNCH: [BootKind; 2] = [BootKind::Launch, BootKind::Resume];

    fn facts(sqlite: bool, vault_password_set: bool, has_encrypted_tokens: bool) -> StorageFacts {
        StorageFacts {
            sqlite,
            vault_password_set,
            has_encrypted_tokens,
        }
    }

    #[test]
    fn a_fresh_sqlite_install_without_a_password_uses_the_keyring() {
        for kind in LAUNCH {
            assert_eq!(
                choose_storage(kind, facts(true, false, false)),
                StorageChoice::Keyring
            );
        }
    }

    #[test]
    fn encrypted_tokens_or_postgres_without_a_password_start_locked() {
        for kind in LAUNCH {
            assert_eq!(
                choose_storage(kind, facts(true, false, true)),
                StorageChoice::Locked
            );
            assert_eq!(
                choose_storage(kind, facts(false, false, false)),
                StorageChoice::Locked
            );
        }
    }

    #[test]
    fn a_password_selects_the_encrypted_store() {
        for kind in LAUNCH {
            assert_eq!(
                choose_storage(kind, facts(true, true, false)),
                StorageChoice::EncryptedWithKeyringFallback
            );
            assert_eq!(
                choose_storage(kind, facts(true, true, true)),
                StorageChoice::EncryptedWithKeyringFallback
            );
            assert_eq!(
                choose_storage(kind, facts(false, true, false)),
                StorageChoice::FromConfig {
                    desktop: false,
                    allow_locked: true
                }
            );
        }
    }

    #[test]
    fn setup_ignores_the_password_for_sqlite_and_keeps_postgres_unlockable() {
        assert_eq!(
            choose_storage(BootKind::Setup, facts(true, true, false)),
            StorageChoice::Keyring
        );
        assert_eq!(
            choose_storage(BootKind::Setup, facts(false, true, false)),
            StorageChoice::FromConfig {
                desktop: true,
                allow_locked: true
            }
        );
    }

    #[test]
    fn unlock_is_always_strict() {
        for sqlite in [true, false] {
            assert_eq!(
                choose_storage(BootKind::Unlock, facts(sqlite, true, true)),
                StorageChoice::FromConfig {
                    desktop: true,
                    allow_locked: false
                }
            );
        }
    }

    #[test]
    fn only_launch_like_boots_recover_and_only_unlocks_announce() {
        assert!(BootKind::Launch.recovers_corruption());
        assert!(BootKind::Resume.recovers_corruption());
        assert!(!BootKind::Setup.recovers_corruption());
        assert!(!BootKind::Unlock.recovers_corruption());
        assert!(BootKind::Unlock.announces_unlock());
        assert!(BootKind::Resume.announces_unlock());
        assert!(!BootKind::Launch.announces_unlock());
        assert!(!BootKind::Setup.announces_unlock());
    }

    #[test]
    fn an_engaged_app_lock_only_boots_for_authenticated_callers() {
        assert!(boot_denied(true, true, false));
        assert!(!boot_denied(true, true, true));
        assert!(!boot_denied(true, false, false));
        assert!(!boot_denied(false, true, false));
    }

    #[test]
    fn corruption_errors_are_told_apart_from_bad_passwords() {
        assert!(is_database_corruption_error(
            "database disk image is malformed"
        ));
        assert!(is_database_corruption_error("File is not a database"));
        assert!(!is_database_corruption_error(
            "failed to decrypt vault password"
        ));
    }

    #[test]
    fn a_config_that_fails_to_load_keeps_the_launch_going_for_the_error_screen() {
        for content in [
            "[storage\nbackend = \"sqlite\"\n",
            "[storage]\ndata_dir = \"${PIPEDASH_TEST_UNSET_VAR_190}\"\n",
        ] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join("config.toml"), content).unwrap();

            let launch = launch_outcome(load_config(dir.path()));
            assert!(launch.is_ok(), "{content}: {launch:?}");

            let status = ConfigLoader::get_setup_status(dir.path());
            assert!(status.config_exists && !status.config_valid, "{content}");
            assert!(!status.validation_errors.is_empty(), "{content}");
        }
    }

    #[test]
    fn a_storage_failure_still_stops_the_launch() {
        let launch = launch_outcome::<()>(Err(BootError::Storage("connection refused".into())));
        assert!(matches!(launch, Err(BootError::Storage(_))));
    }
}
