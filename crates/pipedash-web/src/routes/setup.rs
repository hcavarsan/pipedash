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
use pipedash_core::application::services::session_service::prepare_session;
use pipedash_core::application::services::setup_service::SetupService;
use pipedash_core::domain::setup::{
    SetupApplied,
    SetupDraft,
    SetupDraftResult,
};
use pipedash_core::infrastructure::{
    ConfigLoader,
    PipedashConfig,
    SetupStatus,
    StorageManager,
};
use serde::Serialize;

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

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/status", get(get_setup_status))
        .route("/draft", post(prepare_draft))
        .route("/apply", post(apply_draft))
}

async fn get_setup_status(State(state): State<AppState>) -> Json<SetupStatusResponse> {
    let config_path = ConfigLoader::discover_config_path();
    let data_dir = config_path.parent().unwrap_or(Path::new("."));
    let mut status = ConfigLoader::get_setup_status(data_dir, &state.plugin_registry);

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

async fn prepare_draft(
    State(state): State<AppState>, Json(draft): Json<SetupDraft>,
) -> ApiResult<Json<SetupDraftResult>> {
    ensure_setup_required(&state).await?;

    let service = SetupService::new(discover_config_path().await?, None, false);

    Ok(Json(service.prepare(&draft).await?))
}

async fn apply_draft(
    State(state): State<AppState>, Json(draft): Json<SetupDraft>,
) -> ApiResult<Json<SetupApplied>> {
    let _setup_guard = state.setup_lock.lock().await;
    ensure_setup_required(&state).await?;

    let config_path = discover_config_path().await?;
    let password = draft.vault_password.clone();
    let applied = SetupService::new(config_path.clone(), None, false)
        .apply(draft)
        .await?;

    let config = tokio::task::spawn_blocking(move || ConfigLoader::load(&config_path))
        .await
        .map_err(|e| AppError::internal(format!("Background task failed: {e}")))?
        .map_err(|e| AppError::internal(format!("Failed to load config: {}", e)))?;
    start_core(&state, config).await?;

    if !password.is_empty() && state.api_token().is_none() {
        state.set_api_token(&password);
    }

    Ok(Json(applied))
}

async fn ensure_setup_required(state: &AppState) -> ApiResult<()> {
    if state.inner.read().await.setup_required {
        return Ok(());
    }

    Err(AppError::conflict(
        "Setup already completed. Cannot re-initialize an already-configured instance.",
    ))
}

async fn discover_config_path() -> ApiResult<std::path::PathBuf> {
    tokio::task::spawn_blocking(ConfigLoader::discover_config_path)
        .await
        .map_err(|e| AppError::internal(format!("Background task failed: {e}")))
}

async fn start_core(state: &AppState, config: PipedashConfig) -> ApiResult<()> {
    tracing::info!(
        "Initializing with storage backend: {}",
        config.storage.backend,
    );

    #[cfg(feature = "postgres")]
    if config.storage.backend == pipedash_core::infrastructure::StorageBackendType::Postgres {
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

    let prepared = prepare_session(
        storage_manager,
        state.ws_event_bus.clone(),
        std::sync::Arc::clone(&state.plugin_registry),
    )
    .await
    .map_err(|e| AppError::internal(format!("Failed to initialize core context: {}", e)))?;

    let token_store_ready = prepared
        .warm()
        .await
        .map_err(|e| AppError::internal(format!("Failed to warm up token store: {}", e)))?;

    {
        let mut inner = state.inner.write().await;
        inner.core = Some(prepared.core);
        inner.storage_manager = Some(std::sync::Arc::new(prepared.storage_manager));
        inner.setup_required = false;
        inner.config_error = None;
        inner.token_store_ready = token_store_ready;
    }

    Ok(())
}

#[cfg(test)]
mod tests;
