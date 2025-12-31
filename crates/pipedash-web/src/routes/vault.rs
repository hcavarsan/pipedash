use axum::{
    extract::State,
    routing::{
        get,
        post,
    },
    Json,
    Router,
};
use pipedash_core::infrastructure::StorageBackendType;
use serde::{
    Deserialize,
    Serialize,
};

use crate::error::{
    ApiResult,
    AppError,
};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct VaultStatusResponse {
    pub is_unlocked: bool,
    pub password_source: PasswordSource,
    pub backend: String,
    pub requires_password: bool,
    pub is_first_time: bool,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PasswordSource {
    EnvVar,
    Session,
    None,
}

#[derive(Debug, Deserialize)]
pub struct UnlockVaultRequest {
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct UnlockVaultResponse {
    pub success: bool,
    pub message: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(get_vault_status))
        .route("/unlock", post(unlock_vault))
        .route("/lock", post(lock_vault))
}

async fn get_vault_status(State(state): State<AppState>) -> Json<VaultStatusResponse> {
    let inner = state.inner.read().await;

    let env_var_set = std::env::var("PIPEDASH_VAULT_PASSWORD").is_ok();

    let (backend, requires_password, is_first_time) =
        if let Some(storage_manager) = &inner.storage_manager {
            let config = storage_manager.config();
            let requires = storage_manager.requires_vault_password();
            let first_time = if env_var_set {
                false
            } else {
                storage_manager
                    .is_first_time_vault_setup()
                    .await
                    .unwrap_or(true)
            };
            (config.storage.backend.to_string(), requires, first_time)
        } else {
            ("unknown".to_string(), true, !env_var_set)
        };

    let password_source = determine_password_source(&inner);

    let is_unlocked = inner.token_store_ready;

    Json(VaultStatusResponse {
        is_unlocked,
        password_source,
        backend,
        requires_password,
        is_first_time,
    })
}

fn determine_password_source(inner: &crate::state::AppStateInner) -> PasswordSource {
    if let Some(storage_manager) = &inner.storage_manager {
        let backend = storage_manager.config().storage.backend;

        match backend {
            StorageBackendType::Sqlite | StorageBackendType::Postgres => {
                if std::env::var("PIPEDASH_VAULT_PASSWORD").is_ok() {
                    return PasswordSource::EnvVar;
                }
                if inner.token_store_ready {
                    return PasswordSource::Session;
                }
                return PasswordSource::None;
            }
        }
    }

    PasswordSource::None
}

async fn unlock_vault(
    State(state): State<AppState>, Json(req): Json<UnlockVaultRequest>,
) -> ApiResult<Json<UnlockVaultResponse>> {
    // Check if already unlocked BEFORE setting env var
    {
        let inner = state.inner.read().await;
        if inner.token_store_ready {
            return Ok(Json(UnlockVaultResponse {
                success: true,
                message: "Vault is already unlocked".to_string(),
            }));
        }
    }

    // Only set password after confirming vault needs unlocking
    std::env::set_var("PIPEDASH_VAULT_PASSWORD", &req.password);

    let inner = state.inner.read().await;

    let storage_manager = inner
        .storage_manager
        .as_ref()
        .ok_or_else(AppError::not_initialized)?;

    let config = storage_manager.config().clone();
    let is_desktop = false; // API server is never desktop mode
    drop(inner); // Release lock before potentially long operation

    match pipedash_core::infrastructure::StorageManager::from_config(config.clone(), is_desktop)
        .await
    {
        Ok(new_manager) => {
            let event_bus = {
                let inner = state.inner.read().await;
                inner
                    .core
                    .as_ref()
                    .map(|c| c.event_bus.clone())
                    .unwrap_or_else(|| std::sync::Arc::new(pipedash_core::NoOpEventBus))
            };

            match pipedash_core::CoreContext::with_storage_manager(&new_manager, event_bus.clone())
                .await
            {
                Ok(new_core) => {
                    {
                        let inner = state.inner.read().await;
                        if let Some(old_core) = inner.core.as_ref() {
                            old_core.shutdown().await;
                        }
                    }

                    {
                        let mut inner = state.inner.write().await;
                        inner.storage_manager = Some(new_manager);
                        inner.core = Some(new_core);
                        inner.token_store_ready = true;
                    }

                    {
                        let inner = state.inner.read().await;
                        if let Some(core) = inner.core.as_ref() {
                            if let Err(e) = core.warmup_token_store().await {
                                tracing::warn!(
                                    "Token store warmup warning: {}. Providers may need manual refresh.",
                                    e
                                );
                            }
                            core.start_background_tasks().await;
                        }
                    }

                    tracing::info!(
                        "Vault unlocked successfully via session password - providers reloaded"
                    );

                    Ok(Json(UnlockVaultResponse {
                        success: true,
                        message: "Vault unlocked successfully".to_string(),
                    }))
                }
                Err(e) => {
                    std::env::remove_var("PIPEDASH_VAULT_PASSWORD");

                    tracing::warn!("Failed to unlock vault: {}", e);

                    Ok(Json(UnlockVaultResponse {
                        success: false,
                        message: format!("Failed to unlock vault: {}", e),
                    }))
                }
            }
        }
        Err(e) => {
            std::env::remove_var("PIPEDASH_VAULT_PASSWORD");

            tracing::warn!(
                "Failed to create storage manager with provided password: {}",
                e
            );

            Ok(Json(UnlockVaultResponse {
                success: false,
                message: format!("Invalid password or storage error: {}", e),
            }))
        }
    }
}

async fn lock_vault(State(state): State<AppState>) -> Json<UnlockVaultResponse> {
    std::env::remove_var("PIPEDASH_VAULT_PASSWORD");

    {
        let mut inner = state.inner.write().await;
        inner.token_store_ready = false;
    }

    tracing::info!("Vault locked - session password cleared");

    Json(UnlockVaultResponse {
        success: true,
        message: "Vault locked. Restart required to fully clear token cache.".to_string(),
    })
}
