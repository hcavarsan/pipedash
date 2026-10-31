use axum::{
    extract::State,
    http::StatusCode,
    routing::{
        get,
        post,
        put,
    },
    Json,
    Router,
};
use pipedash_core::infrastructure::{
    ConfigAnalysis,
    ConfigLoader,
    MigrationOptions,
    MigrationOrchestrator,
    MigrationPlan,
    MigrationResult,
    PipedashConfig,
    Platform,
    StorageBackendType,
    StorageConfig,
    StorageManager,
    ValidationReport,
};
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
pub struct StorageConfigResponse {
    pub config: PipedashConfig,
    pub summary: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStorageConfigRequest {
    #[serde(default)]
    pub backend: Option<StorageBackendType>,
    #[serde(default)]
    pub postgres_connection_string: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PlanMigrationRequest {
    pub target_config: PipedashConfig,
    #[serde(default)]
    pub options: MigrationOptions,
}

#[derive(Debug, Deserialize)]
pub struct ExecuteMigrationRequest {
    pub plan: MigrationPlan,
    pub options: MigrationOptions,
}

#[derive(Debug, Serialize)]
pub struct ConfigContentResponse {
    pub content: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveConfigContentRequest {
    pub content: String,
}

#[derive(Debug, Deserialize)]
pub struct AnalyzeConfigRequest {
    pub new_content: String,
}

#[derive(Debug, Serialize)]
pub struct StoragePathsResponse {
    pub config_file: String,
    pub pipedash_db: String,
    pub metrics_db: String,
    pub data_dir: String,
    pub cache_dir: String,
    pub vault_path: String,
}

#[derive(Debug, Serialize)]
pub struct DatabaseExistsResponse {
    pub exists: bool,
    pub path: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/config", get(get_storage_config))
        .route("/config", put(update_storage_config))
        .route("/config/save", post(save_storage_config))
        .route("/config/content", get(get_config_content))
        .route("/config/content", put(save_config_content))
        .route("/config/analyze", post(analyze_config))
        .route("/paths", get(get_storage_paths))
        .route("/default-data-dir", get(get_default_data_dir))
        .route("/effective-data-dir", post(get_effective_data_dir))
        .route("/check-database", post(check_database_exists))
        .route("/validate", post(validate_storage_config))
        .route("/test-connection", post(test_storage_connection))
        .route("/vault-password-status", get(get_vault_password_status))
        .route("/migration/plan", post(plan_migration))
        .route("/migration/execute", post(execute_migration))
}

#[derive(Debug, Serialize)]
pub struct VaultPasswordStatus {
    pub is_set: bool,
    pub env_var_name: &'static str,
}

async fn get_vault_password_status() -> Json<VaultPasswordStatus> {
    let is_set = std::env::var("PIPEDASH_VAULT_PASSWORD").is_ok();
    Json(VaultPasswordStatus {
        is_set,
        env_var_name: "PIPEDASH_VAULT_PASSWORD",
    })
}

async fn get_storage_config(
    State(state): State<AppState>,
) -> ApiResult<Json<StorageConfigResponse>> {
    let inner = state.inner.read().await;
    let storage_manager = inner.storage_manager.as_ref().ok_or_else(|| {
        AppError::not_found("Storage configuration not found - initial setup required")
    })?;

    let mut config = storage_manager.config().clone();
    if let Some(core) = &inner.core {
        config.general = core.config_state.get().await.general.clone();
    }
    let summary = config.storage.summary();

    Ok(Json(StorageConfigResponse { config, summary }))
}

async fn get_config_content(
    State(_state): State<AppState>,
) -> ApiResult<Json<ConfigContentResponse>> {
    use pipedash_core::infrastructure::config::ConfigLoader;

    let config_path = ConfigLoader::discover_config_path();

    tracing::debug!("Reading config file from: {:?}", config_path);

    if !config_path.exists() {
        return Err(AppError::internal(format!(
            "Config file does not exist at: {}",
            config_path.display()
        )));
    }

    let content = std::fs::read_to_string(&config_path).map_err(|e| {
        AppError::internal(format!(
            "Failed to read config file at {}: {}",
            config_path.display(),
            e
        ))
    })?;

    Ok(Json(ConfigContentResponse {
        content,
        path: config_path.display().to_string(),
    }))
}

async fn save_config_content(
    State(state): State<AppState>, Json(req): Json<SaveConfigContentRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    use pipedash_core::infrastructure::config::ConfigLoader;

    let config: PipedashConfig = toml::from_str(&req.content)
        .map_err(|e| AppError::bad_request(format!("Invalid TOML syntax: {}", e)))?;

    validate_config(&config)?;

    let config_path = ConfigLoader::discover_config_path();

    std::fs::write(&config_path, &req.content).map_err(|e| {
        AppError::internal(format!(
            "Failed to save config file at {}: {}",
            config_path.display(),
            e
        ))
    })?;

    tracing::info!("Config file saved: {:?}", config_path);

    let inner = state.inner.read().await;
    if let Some(core) = &inner.core {
        core.apply_config_file().await.map_err(|e| {
            AppError::internal(format!(
                "config.toml was saved, but its providers couldn't be applied: {e}"
            ))
        })?;
    }

    Ok(Json(serde_json::json!({"success": true})))
}

fn validate_config(config: &PipedashConfig) -> ApiResult<()> {
    let validation = config.validate();
    if !validation.is_ok() {
        let errors: Vec<String> = validation
            .errors
            .iter()
            .map(|e| format!("{:?}", e))
            .collect();
        return Err(AppError::bad_request(format!(
            "Config validation failed: {}",
            errors.join(", ")
        )));
    }

    Ok(())
}

async fn analyze_config(
    State(state): State<AppState>, Json(req): Json<AnalyzeConfigRequest>,
) -> ApiResult<Json<ConfigAnalysis>> {
    let inner = state.inner.read().await;
    let storage_manager = inner
        .storage_manager
        .as_ref()
        .ok_or_else(|| AppError::not_found("Storage configuration not found"))?;
    let event_bus = inner.core.as_ref().map(|core| core.event_bus.clone());

    let analysis = pipedash_core::infrastructure::analyze_config(
        &req.new_content,
        storage_manager,
        event_bus,
        None,
    )
    .await?;

    Ok(Json(analysis))
}

async fn get_storage_paths(State(state): State<AppState>) -> ApiResult<Json<StoragePathsResponse>> {
    use pipedash_core::infrastructure::config::ConfigLoader;

    let inner = state.inner.read().await;
    let storage_manager = inner
        .storage_manager
        .as_ref()
        .ok_or_else(|| AppError::internal("Storage manager not available"))?;

    let data_dir = storage_manager.config().data_dir();

    let config_path = ConfigLoader::discover_config_path();

    Ok(Json(StoragePathsResponse {
        config_file: config_path.display().to_string(),
        pipedash_db: data_dir.join("pipedash.db").display().to_string(),
        metrics_db: data_dir.join("metrics.db").display().to_string(),
        data_dir: data_dir.display().to_string(),
        cache_dir: data_dir.join("cache").display().to_string(),
        vault_path: data_dir.join("vault").display().to_string(),
    }))
}

async fn get_default_data_dir() -> Json<String> {
    Json(PipedashConfig::default_data_dir().display().to_string())
}

async fn get_effective_data_dir(Json(config): Json<PipedashConfig>) -> Json<String> {
    Json(config.data_dir().display().to_string())
}

async fn check_database_exists(Json(config): Json<PipedashConfig>) -> Json<DatabaseExistsResponse> {
    let db_path = config.db_path();

    Json(DatabaseExistsResponse {
        exists: db_path.exists(),
        path: db_path.display().to_string(),
    })
}

async fn validate_storage_config(
    State(state): State<AppState>, Json(config): Json<StorageConfig>,
) -> ApiResult<Json<ValidationReport>> {
    let inner = state.inner.read().await;
    let storage_manager = inner
        .storage_manager
        .as_ref()
        .ok_or_else(|| AppError::internal("Storage manager not available"))?;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;

    let orchestrator = MigrationOrchestrator::from_manager(
        storage_manager,
        Some(core.event_bus.clone()),
        None, // API mode: uses from_config, not with_token_store
    )
    .await;

    let report = orchestrator
        .validate_target_config(&config)
        .await
        .map_err(|e| AppError::internal(format!("Failed to validate config: {}", e)))?;

    Ok(Json(report))
}

async fn test_storage_connection(
    State(_state): State<AppState>, Json(config): Json<PipedashConfig>,
) -> ApiResult<Json<serde_json::Value>> {
    match StorageManager::from_config(config, false).await {
        Ok(_manager) => Ok(Json(serde_json::json!({
            "success": true,
            "message": "Connection successful! All backends are accessible."
        }))),
        Err(e) => Ok(Json(serde_json::json!({
            "success": false,
            "message": format!("Connection failed: {}", e)
        }))),
    }
}

async fn save_storage_config(
    State(state): State<AppState>, Json(config): Json<PipedashConfig>,
) -> ApiResult<StatusCode> {
    validate_config(&config)?;

    let config_path = ConfigLoader::discover_config_path();
    let previous = std::fs::read_to_string(&config_path).ok();
    ConfigLoader::save(&config, &config_path).map_err(|e| {
        AppError::internal(format!(
            "Failed to save storage config at {}: {}",
            config_path.display(),
            e
        ))
    })?;
    tracing::info!("Storage config saved: {}", config.storage.summary());

    if let Err(e) = reload_storage(&state).await {
        let restored = match &previous {
            Some(content) => std::fs::write(&config_path, content),
            None => std::fs::remove_file(&config_path),
        };
        if let Err(restore_error) = restored {
            tracing::error!(
                "Failed to restore {} after a failed storage switch: {}",
                config_path.display(),
                restore_error
            );
        }
        return Err(AppError::internal(e));
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn update_storage_config(
    State(state): State<AppState>, Json(req): Json<UpdateStorageConfigRequest>,
) -> ApiResult<Json<MigrationPlan>> {
    let inner = state.inner.read().await;
    let storage_manager = inner
        .storage_manager
        .as_ref()
        .ok_or_else(|| AppError::internal("Storage manager not available"))?;

    let current_config = storage_manager.config().clone();

    let mut target_config = current_config.clone();
    if let Some(backend) = req.backend {
        target_config.storage.backend = backend;
    }
    if let Some(connection_string) = req.postgres_connection_string {
        target_config.storage.postgres.connection_string = connection_string;
    }

    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let orchestrator = MigrationOrchestrator::from_manager(
        storage_manager,
        Some(core.event_bus.clone()),
        None, // API mode: uses from_config, not with_token_store
    )
    .await;

    let plan = orchestrator
        .plan_migration(target_config, &MigrationOptions::default())
        .map_err(|e| AppError::internal(format!("Failed to plan migration: {}", e)))?;

    Ok(Json(plan))
}

async fn plan_migration(
    State(state): State<AppState>, Json(req): Json<PlanMigrationRequest>,
) -> ApiResult<Json<MigrationPlan>> {
    let inner = state.inner.read().await;
    let storage_manager = inner
        .storage_manager
        .as_ref()
        .ok_or_else(|| AppError::internal("Storage manager not available"))?;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;

    let orchestrator = MigrationOrchestrator::from_manager(
        storage_manager,
        Some(core.event_bus.clone()),
        None, // API mode: uses from_config, not with_token_store
    )
    .await;

    let plan = orchestrator
        .plan_migration(req.target_config, &req.options)
        .map_err(|e| AppError::internal(format!("Failed to plan migration: {}", e)))?;

    Ok(Json(plan))
}

async fn execute_migration(
    State(state): State<AppState>, Json(req): Json<ExecuteMigrationRequest>,
) -> ApiResult<Json<MigrationResult>> {
    let token_password = req.options.token_password.clone();

    let result = {
        let inner = state.inner.read().await;
        let storage_manager = inner
            .storage_manager
            .as_ref()
            .ok_or_else(|| AppError::internal("Storage manager not available"))?;
        let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;

        let orchestrator = MigrationOrchestrator::from_manager(
            storage_manager,
            Some(core.event_bus.clone()),
            None, // API mode: uses from_config, not with_token_store
        )
        .await;

        orchestrator
            .execute_migration(req.plan, req.options, false)
            .await
    }; // inner and orchestrator dropped here, releasing database connections

    if result.success {
        if let Some(password) = &token_password {
            std::env::set_var("PIPEDASH_VAULT_PASSWORD", password);
            state.set_api_token(password);
            tracing::info!("Vault password set in environment for this session");
        }

        if let Err(e) = reload_storage(&state).await {
            tracing::error!("{} after migration. Manual restart required.", e);
        }
    }

    Ok(Json(result))
}

async fn reload_storage(state: &AppState) -> Result<(), String> {
    let config_path = ConfigLoader::discover_config_path();
    let config = ConfigLoader::load_or_create(&config_path, Platform::Server)
        .map_err(|e| format!("Failed to reload config: {}", e))?;
    tracing::info!(
        "Reloading storage manager with new config: {}",
        config.storage.summary()
    );

    let storage_manager = StorageManager::from_config(config, false)
        .await
        .map_err(|e| format!("Failed to create new storage manager: {}", e))?;
    let new_core = pipedash_core::CoreContext::with_storage_manager(
        &storage_manager,
        state.ws_event_bus.clone(),
    )
    .await
    .map_err(|e| format!("Failed to recreate CoreContext: {}", e))?;

    let old_core = {
        let mut inner = state.inner.write().await;
        inner.storage_manager = Some(storage_manager);
        inner.core.replace(new_core)
    };
    if let Some(old_core) = old_core {
        state.download_tickets.clear();
        old_core.shutdown().await;
    }

    let inner = state.inner.read().await;
    if let Some(core) = inner.core.as_ref() {
        if let Err(e) = core.warmup_token_store().await {
            tracing::warn!(
                "Token store warmup warning: {}. Providers may need manual refresh.",
                e
            );
        }
        drop(core.start_background_tasks());
    }
    tracing::info!("Storage reloaded - token store warmed up and background tasks started");

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{
        header,
        Request,
        StatusCode,
    };
    use pipedash_core::{
        CoreContext,
        ProviderConfig,
    };
    use tower::ServiceExt;

    use super::*;
    use crate::ws::WebSocketEventBus;

    async fn call(request: Request<Body>) -> (StatusCode, serde_json::Value) {
        let app = router().with_state(AppState::setup_mode(Arc::new(WebSocketEventBus::new())));
        let response = app.oneshot(request).await.unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();

        (status, serde_json::from_slice(&body).unwrap())
    }

    fn post_config(uri: &str, data_dir: &std::path::Path) -> Request<Body> {
        let mut config = PipedashConfig::default();
        config.storage.data_dir = data_dir.display().to_string();

        Request::post(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&config).unwrap()))
            .unwrap()
    }

    #[tokio::test]
    async fn default_data_dir_returns_a_json_string() {
        let request = Request::get("/default-data-dir")
            .body(Body::empty())
            .unwrap();

        let (status, body) = call(request).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            serde_json::json!(PipedashConfig::default_data_dir().display().to_string())
        );
    }

    #[tokio::test]
    async fn effective_data_dir_and_check_database_follow_the_posted_config() {
        let data_dir =
            std::env::temp_dir().join(format!("pipedash-storage-routes-{}", std::process::id()));
        std::fs::create_dir_all(&data_dir).unwrap();
        let db_path = data_dir.join("pipedash.db").display().to_string();

        let (status, effective) = call(post_config("/effective-data-dir", &data_dir)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(effective, serde_json::json!(data_dir.display().to_string()));

        let (_, missing) = call(post_config("/check-database", &data_dir)).await;
        std::fs::write(&db_path, b"").unwrap();
        let (_, present) = call(post_config("/check-database", &data_dir)).await;
        let _ = std::fs::remove_dir_all(&data_dir);

        assert_eq!(
            missing,
            serde_json::json!({ "exists": false, "path": db_path })
        );
        assert_eq!(
            present,
            serde_json::json!({ "exists": true, "path": db_path })
        );
    }

    async fn save_from_store_with_provider(
        old_dir: &std::path::Path, target_data_dir: &std::path::Path,
    ) -> (StatusCode, usize, String) {
        std::fs::create_dir_all(old_dir).unwrap();
        let config_path = old_dir.join("config.toml");
        std::fs::write(
            &config_path,
            format!(
                "[storage]\nbackend = \"sqlite\"\ndata_dir = '{}'\nvault_password = \"vault-password\"\n",
                old_dir.display()
            ),
        )
        .unwrap();
        std::env::set_var("PIPEDASH_CONFIG_PATH", &config_path);

        let storage_manager =
            StorageManager::from_config(ConfigLoader::load(&config_path).unwrap(), false)
                .await
                .unwrap();
        let provider = ProviderConfig {
            id: None,
            name: "ci".into(),
            provider_type: "jenkins".into(),
            token: String::new(),
            config: [("server_url".to_string(), "http://127.0.0.1:9".to_string())].into(),
            refresh_interval: 30,
            version: None,
        };
        storage_manager
            .config_backend()
            .create_provider(&provider)
            .await
            .unwrap();
        let ws_event_bus = Arc::new(WebSocketEventBus::new());
        let core = CoreContext::with_storage_manager(&storage_manager, ws_event_bus.clone())
            .await
            .unwrap();
        let state = AppState::initialized(ws_event_bus, core, storage_manager);

        let mut target = PipedashConfig::default();
        target.storage.data_dir = target_data_dir.display().to_string();
        let request = Request::post("/config/save")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&target).unwrap()))
            .unwrap();
        let status = router()
            .with_state(state.clone())
            .oneshot(request)
            .await
            .unwrap()
            .status();
        let provider_service = state.provider_service().await.ok().unwrap();
        let providers = provider_service.list_providers().await.unwrap();
        let saved = ConfigLoader::load(&config_path).unwrap();

        (status, providers.len(), saved.storage.data_dir)
    }

    #[tokio::test]
    async fn save_config_switches_to_an_empty_store_without_copying_providers() {
        let _env = crate::routes::CONFIG_ENV.lock().await;
        let root =
            std::env::temp_dir().join(format!("pipedash-save-storage-{}", std::process::id()));
        let new_dir = root.join("new");

        let (status, providers, saved_dir) =
            save_from_store_with_provider(&root.join("old"), &new_dir).await;
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(providers, 0);
        assert_eq!(saved_dir, new_dir.display().to_string());
    }

    #[tokio::test]
    async fn save_config_keeps_the_old_config_when_the_new_store_cannot_open() {
        let _env = crate::routes::CONFIG_ENV.lock().await;
        let root =
            std::env::temp_dir().join(format!("pipedash-save-storage-bad-{}", std::process::id()));
        let old_dir = root.join("old");
        let unopenable = old_dir.join("config.toml").join("store");

        let (status, providers, saved_dir) =
            save_from_store_with_provider(&old_dir, &unopenable).await;
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(providers, 1);
        assert_eq!(saved_dir, old_dir.display().to_string());
    }
}
