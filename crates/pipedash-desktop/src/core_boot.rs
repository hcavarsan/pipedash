pub(crate) mod background;
mod recovery;

use std::path::Path;
use std::sync::Arc;

use pipedash_core::application::services::session_service::prepare_session;
use pipedash_core::application::services::storage_service::config as storage_config;
#[cfg(desktop)]
use pipedash_core::application::services::storage_service::vault::{
    requires_vault_password,
    vault_is_unlocked,
};
use pipedash_core::infrastructure::config::PipedashConfig;
use pipedash_core::infrastructure::database::has_encrypted_tokens;
use pipedash_core::CoreContext;
use tauri::{
    AppHandle,
    Manager,
};

use self::background::BackgroundInit;
use self::recovery::create_storage_manager;
use crate::app_lock::AppLockState;
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
    let config = load_config(&app_data_dir).await?;

    let facts = StorageFacts {
        sqlite: config.storage.backend.is_sqlite(),
        vault_password_set: std::env::var("PIPEDASH_VAULT_PASSWORD").is_ok(),
        has_encrypted_tokens: has_encrypted_tokens(&config.data_dir().join("pipedash.db")).await,
    };
    let choice = choose_storage(kind, facts);
    log_choice(choice, facts);

    let storage_manager = create_storage_manager(kind, choice, &config).await?;

    if kind == BootKind::Unlock {
        match storage_manager.token_store().await.get_all_tokens().await {
            Ok(tokens) => tracing::info!(
                "Password validated - decrypted {} tokens successfully",
                tokens.len()
            ),
            Err(e) => return Err(BootError::Decrypt(e.to_string())),
        }
    }

    #[cfg(desktop)]
    let generation = crate::desktop::new_generation(app);
    #[cfg(mobile)]
    let generation = 0;
    let event_bus = create_tauri_event_bus(app.clone(), generation);
    let session = prepare_session(
        storage_manager,
        event_bus,
        crate::plugin_registry::plugin_registry(),
    )
    .await
    .map_err(|e| BootError::Context(e.to_string()))?;
    let core = Arc::new(session.core);

    #[cfg(desktop)]
    {
        crate::desktop::projection::detach(app);
        crate::desktop::flush_menu(app).await;
    }
    let maybe_core = app.state::<MaybeCoreContext>();
    background::cancel_init(app).await;
    let old = maybe_core.0.write().await.replace(Arc::clone(&core));
    if let Some(old) = old {
        #[cfg(mobile)]
        crate::build_watch::interrupt(app);
        old.shutdown().await;
    }
    lock.set_locked(false);
    crate::host::core_changed(app, Some(&core)).await;

    #[cfg(desktop)]
    {
        let vault_ready = choice != StorageChoice::Locked
            && vault_is_unlocked(
                requires_vault_password(&config).await,
                std::env::var("PIPEDASH_VAULT_PASSWORD").is_ok(),
                lock.locked(),
            );
        crate::desktop::projection::attach(app, Arc::clone(&core), generation, vault_ready).await;
    }

    if choice == StorageChoice::Locked {
        tracing::info!(
            "Vault locked - background tasks deferred until unlock. \
             Providers will not be loaded until vault is unlocked."
        );
    } else {
        app.state::<BackgroundInit>().start(Arc::clone(&core), kind);
    }

    tracing::info!("Pipedash initialized with CoreContext ({:?})", kind);
    crate::local_notifications::settings::schedule_sync(app);
    Ok(core)
}

async fn load_config(app_data_dir: &Path) -> Result<PipedashConfig, BootError> {
    storage_config::load_config(app_data_dir)
        .await
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

#[cfg(test)]
mod tests;
