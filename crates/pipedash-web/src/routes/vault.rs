use std::sync::Arc;

use axum::{
    extract::State,
    routing::{
        get,
        post,
    },
    Json,
    Router,
};
use pipedash_core::application::services::session_service::prepare_session;
use pipedash_core::application::services::storage_service::vault::{
    PasswordSource,
    UnlockVaultResponse,
    VaultStatus,
};
use pipedash_core::domain::{
    DomainError,
    DomainResult,
};
use pipedash_core::event::CoreEvent;
use pipedash_core::infrastructure::migration::VAULT_PASSWORD_ENV_VAR;
use pipedash_core::infrastructure::StorageManager;
use serde::{
    Deserialize,
    Serialize,
};

use super::storage::session::install_session;
use crate::error::{
    ApiResult,
    AppError,
};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct VaultStatusResponse {
    #[serde(flatten)]
    pub status: VaultStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub demo_password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UnlockVaultRequest {
    pub password: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(get_vault_status))
        .route("/unlock", post(unlock_vault))
        .route("/lock", post(lock_vault))
}

async fn get_vault_status(State(state): State<AppState>) -> Json<VaultStatusResponse> {
    if state.desktop_host.is_some() {
        let config_state = state
            .inner
            .read()
            .await
            .core
            .as_ref()
            .map(|core| Arc::clone(&core.config_state));
        let backend = match &config_state {
            Some(config_state) => config_state.get().await.storage.backend.to_string(),
            None => "unknown".to_string(),
        };

        return Json(VaultStatusResponse {
            status: VaultStatus {
                is_unlocked: config_state.is_some(),
                password_source: PasswordSource::None,
                backend,
                requires_password: false,
                is_first_time: false,
            },
            demo_password: None,
        });
    }

    let (storage_manager, token_store_ready) = {
        let inner = state.inner.read().await;
        (inner.storage_manager.clone(), inner.token_store_ready)
    };

    let (backend, requires_password, is_first_time) = match &storage_manager {
        Some(storage_manager) => {
            let first_time = storage_manager
                .is_first_time_vault_setup()
                .await
                .unwrap_or(true);
            (
                storage_manager.config().storage.backend.to_string(),
                storage_manager.requires_vault_password(),
                first_time,
            )
        }
        None => (
            "unknown".to_string(),
            true,
            std::env::var(VAULT_PASSWORD_ENV_VAR).is_err(),
        ),
    };

    Json(VaultStatusResponse {
        status: VaultStatus {
            is_unlocked: token_store_ready,
            password_source: determine_password_source(
                storage_manager.is_some(),
                token_store_ready,
            ),
            backend,
            requires_password,
            is_first_time,
        },
        demo_password: crate::demo::enabled()
            .then(|| state.api_token().map(|token| token.to_string()))
            .flatten(),
    })
}

fn determine_password_source(has_storage: bool, token_store_ready: bool) -> PasswordSource {
    if !has_storage {
        return PasswordSource::None;
    }
    if std::env::var(VAULT_PASSWORD_ENV_VAR).is_ok() {
        return PasswordSource::EnvVar;
    }
    if token_store_ready {
        return PasswordSource::Session;
    }

    PasswordSource::None
}

fn unlock_response(success: bool, message: impl Into<String>) -> Json<UnlockVaultResponse> {
    Json(if success {
        UnlockVaultResponse::ok(message)
    } else {
        UnlockVaultResponse::failed(message)
    })
}

async fn install_unlocked(
    state: &AppState, new_manager: StorageManager, password: &str,
) -> DomainResult<()> {
    let session = prepare_session(
        new_manager,
        state.ws_event_bus.clone(),
        Arc::clone(&state.plugin_registry),
    )
    .await?;
    install_session(state, session).await?;
    std::env::set_var(VAULT_PASSWORD_ENV_VAR, password);
    state.set_api_token(password);

    Ok(())
}

async fn unlock_vault(
    State(state): State<AppState>, Json(req): Json<UnlockVaultRequest>,
) -> ApiResult<Json<UnlockVaultResponse>> {
    let (storage_manager, ready) = {
        let inner = state.inner.read().await;
        (inner.storage_manager.clone(), inner.token_store_ready)
    };
    if ready {
        return Ok(unlock_response(true, "Vault is already unlocked"));
    }
    let storage_manager = storage_manager.ok_or_else(AppError::not_initialized)?;

    let mut config = storage_manager.config().clone();
    config.storage.vault_password = Some(req.password.clone());
    let outcome = match StorageManager::from_config(config, false).await {
        Ok(new_manager) => install_unlocked(&state, new_manager, &req.password)
            .await
            .map(|()| "Vault unlocked successfully".to_string())
            .map_err(|e| format!("Failed to unlock vault: {e}")),
        Err(DomainError::AuthenticationFailed(e)) => {
            tracing::warn!("Vault unlock rejected: {}", e);
            Err("Incorrect vault password".to_string())
        }
        Err(e) => {
            tracing::warn!("Failed to create storage manager with provided password: {e}");
            Err(format!("Invalid password or storage error: {e}"))
        }
    };
    if let Err(message) = &outcome {
        tracing::warn!("Vault unlock failed: {message}");
    }

    Ok(match outcome {
        Ok(message) => unlock_response(true, message),
        Err(message) => unlock_response(false, message),
    })
}

async fn lock_vault(State(state): State<AppState>) -> Json<UnlockVaultResponse> {
    std::env::remove_var(VAULT_PASSWORD_ENV_VAR);

    let old_core = {
        let mut inner = state.inner.write().await;
        inner.token_store_ready = false;
        inner.core.take()
    };
    if let Some(old_core) = old_core {
        state.download_tickets.clear();
        old_core.event_bus.emit(CoreEvent::VaultLocked).await;
        old_core.shutdown().await;
    }

    tracing::info!("Vault locked - core shut down and session password cleared");

    Json(UnlockVaultResponse::ok("Vault locked"))
}
