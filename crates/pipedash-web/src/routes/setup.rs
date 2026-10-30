use std::path::Path;

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

async fn get_setup_status(State(state): State<AppState>) -> Json<SetupStatusResponse> {
    let config_path = ConfigLoader::discover_config_path();
    let data_dir = config_path.parent().unwrap_or(Path::new("."));
    let mut status = ConfigLoader::get_setup_status(data_dir);

    if status.config_valid && state.inner.read().await.config_error.is_some() {
        if let Err(err) = start_core_from_fixed_config(&state, &config_path).await {
            tracing::error!(
                "Failed to start with the fixed config: {}",
                err.error.message
            );
            status.config_valid = false;
            status.validation_errors = vec![err.error.message];
        }
    }

    Json(status.into())
}

async fn start_core_from_fixed_config(state: &AppState, config_path: &Path) -> ApiResult<()> {
    let _setup_guard = state.setup_lock.lock().await;
    if state.inner.read().await.config_error.is_none() {
        return Ok(());
    }

    tracing::info!("Configuration fixed - initializing services");
    let config = ConfigLoader::load(config_path)
        .map_err(|e| AppError::internal(format!("Failed to load config: {}", e)))?;

    start_core(state, config).await
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

    start_core(&state, config).await?;

    if let Some(password) = req.vault_password.as_deref() {
        if state.api_token().is_none() {
            state.set_api_token(password);
        }
    }

    tracing::info!("Application state updated - setup complete!");

    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Configuration created and system initialized successfully",
        "config_path": config_path.to_string_lossy()
    })))
}

async fn start_core(state: &AppState, config: PipedashConfig) -> ApiResult<()> {
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

    let storage_manager = StorageManager::from_config_allow_locked(config, false)
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

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{
        Request,
        StatusCode,
    };
    use tower::ServiceExt;

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
        let _env = crate::routes::CONFIG_ENV.lock().await;
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

    async fn get_json(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let response = app
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();

        (status, serde_json::from_slice(&body).unwrap_or_default())
    }

    #[tokio::test]
    async fn status_check_after_fixing_the_config_starts_the_services() {
        let _env = crate::routes::CONFIG_ENV.lock().await;
        let data_dir =
            std::env::temp_dir().join(format!("pipedash-config-fixed-{}", std::process::id()));
        let config_path = data_dir.join("config.toml");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::write(&config_path, "[storage\nbackend = \"sqlite\"\n").unwrap();
        std::env::set_var("PIPEDASH_CONFIG_PATH", &config_path);
        let app = Router::new()
            .nest("/setup", router())
            .nest("/providers", crate::routes::providers::router())
            .with_state(AppState::config_error(
                Arc::new(WebSocketEventBus::new()),
                "invalid".into(),
            ));

        let (broken_status, broken_body) = get_json(&app, "/setup/status").await;
        let mut config = PipedashConfig::default();
        config.storage.backend = StorageBackendType::Sqlite;
        config.storage.data_dir = data_dir.display().to_string();
        config.storage.vault_password = Some("vault-password".into());
        std::fs::write(&config_path, ConfigLoader::to_toml(&config).unwrap()).unwrap();
        let (fixed_status, fixed_body) = get_json(&app, "/setup/status").await;
        let (providers_status, providers_body) = get_json(&app, "/providers").await;
        let _ = std::fs::remove_dir_all(&data_dir);

        assert_eq!(broken_status, StatusCode::OK);
        assert_eq!(broken_body["config_valid"], false, "{broken_body}");
        assert_eq!(fixed_status, StatusCode::OK);
        assert_eq!(fixed_body["config_valid"], true, "{fixed_body}");
        assert_eq!(providers_status, StatusCode::OK, "{providers_body}");
    }
}
