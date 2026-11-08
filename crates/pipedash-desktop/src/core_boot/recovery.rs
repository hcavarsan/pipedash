use std::sync::Arc;

use pipedash_core::application::services::storage_service::recovery::{
    build_with_recovery,
    RecoveryFailure,
};
use pipedash_core::domain::{
    DomainError,
    DomainResult,
};
use pipedash_core::infrastructure::config::PipedashConfig;
use pipedash_core::infrastructure::database::init_database;
use pipedash_core::infrastructure::secrets::{
    MemoryTokenStore,
    SqliteTokenStore,
};
use pipedash_core::infrastructure::{
    StorageManager,
    TokenStore,
};

use super::{
    BootError,
    BootKind,
    StorageChoice,
};
use crate::fallback_store::FallbackTokenStore;
use crate::keyring_store::KeyringTokenStore;

pub(super) async fn create_storage_manager(
    kind: BootKind, choice: StorageChoice, config: &PipedashConfig,
) -> Result<StorageManager, BootError> {
    build_with_recovery(config, kind.recovers_corruption(), || {
        build_storage_manager(choice, config)
    })
    .await
    .map_err(|failure| match failure {
        RecoveryFailure::Build(error) => unrecoverable(kind, &error.to_string()),
        RecoveryFailure::Recovery(message) => BootError::Storage(message),
    })
}

fn unrecoverable(kind: BootKind, error_msg: &str) -> BootError {
    match kind {
        BootKind::Setup => {
            tracing::error!("StorageManager initialization failed: {}", error_msg);
            BootError::Storage(format!(
                "Failed to initialize StorageManager: {}",
                error_msg
            ))
        }
        BootKind::Unlock => {
            tracing::warn!("Vault unlock failed - invalid password: {}", error_msg);
            BootError::InvalidPassword(error_msg.to_string())
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
