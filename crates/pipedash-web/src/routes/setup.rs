use axum::{
    extract::State,
    routing::{
        get,
        post,
    },
    Json,
    Router,
};
use pipedash_core::infrastructure::{
    ConfigLoader,
    PipedashConfig,
    SetupStatus,
    StorageBackendType,
    StorageManager,
};
use pipedash_core::CoreContext;
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
pub struct SetupStatusResponse {
    pub config_exists: bool,
    pub config_valid: bool,
    pub validation_errors: Vec<String>,
    pub needs_setup: bool,
    pub needs_migration: bool,
}

impl From<SetupStatus> for SetupStatusResponse {
    fn from(status: SetupStatus) -> Self {
        Self {
            config_exists: status.config_exists,
            config_valid: status.config_valid,
            validation_errors: status.validation_errors,
            needs_setup: status.needs_setup,
            needs_migration: status.needs_migration,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateInitialConfigRequest {
    pub config: PipedashConfig,
    #[serde(default)]
    pub vault_password: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(get_setup_status))
        .route("/config", post(create_initial_config))
}

async fn get_setup_status() -> Json<SetupStatusResponse> {
    let config_path = ConfigLoader::discover_config_path();
    let data_dir = config_path.parent().unwrap_or(std::path::Path::new("."));
    let status = ConfigLoader::get_setup_status(data_dir);
    Json(status.into())
}

async fn create_initial_config(
    State(state): State<AppState>, Json(req): Json<CreateInitialConfigRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    // Held for the whole setup so a concurrent request sees the flag this one
    // clears.
    let _setup_guard = state.setup_lock.lock().await;
    if !state.inner.read().await.setup_required {
        return Err(AppError::conflict(
            "Setup already completed. Cannot re-initialize an already-configured instance.",
        ));
    }

    let config = req.config;
    let config_path = ConfigLoader::discover_config_path();

    if let Some(password) = &req.vault_password {
        std::env::set_var("PIPEDASH_VAULT_PASSWORD", password);
        tracing::info!("Vault password set for session (from setup wizard)");
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::internal(format!("Failed to create config directory: {}", e)))?;
    }

    ConfigLoader::save(&config, &config_path)
        .map_err(|e| AppError::internal(format!("Failed to save configuration: {}", e)))?;

    tracing::info!("Initial configuration created at {:?}", config_path);

    tracing::info!(
        "Initializing with storage backend: {}",
        config.storage.backend,
    );

    #[cfg(feature = "postgres")]
    if config.storage.backend == StorageBackendType::Postgres {
        use pipedash_core::infrastructure::database::init_postgres_database;

        tracing::info!("Running PostgreSQL migrations...");

        init_postgres_database(&config.storage.postgres.connection_string)
            .await
            .map_err(|e| AppError::internal(format!("Failed to initialize PostgreSQL: {}", e)))?;

        tracing::info!("PostgreSQL migrations completed");
    }

    let storage_manager = StorageManager::from_config_allow_locked(config.clone(), false)
        .await
        .map_err(|e| AppError::internal(format!("Failed to initialize storage manager: {}", e)))?;

    tracing::info!("Storage manager initialized");

    let vault_locked = storage_manager.is_vault_locked().await;
    if vault_locked {
        tracing::warn!("Vault password not set - setup completing in locked mode");
        tracing::warn!(
            "Set PIPEDASH_VAULT_PASSWORD environment variable or unlock via /api/v1/vault/unlock"
        );
    }

    let event_bus = state.ws_event_bus.clone();
    let core_context = CoreContext::with_storage_manager(&storage_manager, event_bus)
        .await
        .map_err(|e| AppError::internal(format!("Failed to initialize core context: {}", e)))?;

    let token_store_ready = if !vault_locked {
        tracing::info!("Warming up token store after setup completion...");
        core_context
            .warmup_token_store()
            .await
            .map_err(|e| AppError::internal(format!("Failed to warm up token store: {}", e)))?;

        core_context.start_background_tasks().await;
        tracing::info!("Core context initialized and background tasks started");
        true
    } else {
        tracing::info!(
            "Vault locked - skipping token store warmup. Unlock vault to enable providers."
        );
        false
    };

    {
        let mut inner = state.inner.write().await;
        inner.core = Some(core_context);
        inner.storage_manager = Some(storage_manager);
        inner.setup_required = false;
        inner.config_error = None;
        inner.token_store_ready = token_store_ready;
    }
    if let Some(password) = req.vault_password {
        *state.api_token.write().await = Some(password);
    }

    tracing::info!("Application state updated - setup complete!");

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Configuration created and system initialized successfully",
        "config_path": config_path.to_string_lossy()
    })))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::http::StatusCode;

    use super::*;
    use crate::ws::WebSocketEventBus;

    #[tokio::test]
    async fn create_initial_config_rejects_configured_instance() {
        let state = AppState::config_error(Arc::new(WebSocketEventBus::new()), "invalid".into());
        let req = CreateInitialConfigRequest {
            config: PipedashConfig::default(),
            vault_password: Some("attacker".into()),
        };

        let err = create_initial_config(State(state), Json(req))
            .await
            .unwrap_err();

        assert_eq!(err.status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn create_initial_config_rechecks_after_waiting_for_running_setup() {
        // Points a regressed handler at a throwaway path instead of the real
        // config.
        let config_dir =
            std::env::temp_dir().join(format!("pipedash-setup-race-{}", std::process::id()));
        std::env::set_var("PIPEDASH_CONFIG_PATH", config_dir.join("config.toml"));
        let state = AppState::setup_mode(Arc::new(WebSocketEventBus::new()));
        let running_setup = state.setup_lock.clone().lock_owned().await;
        let req = CreateInitialConfigRequest {
            config: PipedashConfig::default(),
            vault_password: None,
        };
        let waiting_setup = tokio::spawn(create_initial_config(State(state.clone()), Json(req)));
        tokio::task::yield_now().await;

        state.inner.write().await.setup_required = false;
        drop(running_setup);
        let result = waiting_setup.await.unwrap();
        let _ = std::fs::remove_dir_all(&config_dir);

        assert_eq!(result.unwrap_err().status, StatusCode::CONFLICT);
    }
}
