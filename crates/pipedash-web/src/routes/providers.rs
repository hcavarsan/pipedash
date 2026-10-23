use std::collections::HashMap;

use axum::{
    extract::{
        Path,
        State,
    },
    routing::{
        delete,
        get,
        post,
        put,
    },
    Json,
    Router,
};
use pipedash_core::application::PipelineService;
use pipedash_core::domain::{
    PaginatedAvailablePipelines,
    PaginationParams,
    ProviderConfig,
    ProviderImportResult,
    RefreshResult,
    RefreshState,
};
use pipedash_plugin_api::{
    FeatureAvailability,
    Organization,
    PermissionStatus,
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

#[derive(Debug, Deserialize)]
pub struct CreateProviderRequest {
    pub name: String,
    pub provider_type: String,
    pub token: String,
    #[serde(default)]
    pub config: HashMap<String, String>,
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval: i64,
}

fn default_refresh_interval() -> i64 {
    60
}

#[derive(Debug, Deserialize)]
pub struct UpdateProviderRequest {
    pub name: String,
    pub token: String,
    #[serde(default)]
    pub config: HashMap<String, String>,
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval: i64,
}

#[derive(Debug, Serialize)]
pub struct ProviderResponse {
    pub id: i64,
    pub name: String,
    pub provider_type: String,
    #[serde(default)]
    pub config: HashMap<String, String>,
    pub refresh_interval: i64,
}

impl From<ProviderConfig> for ProviderResponse {
    fn from(config: ProviderConfig) -> Self {
        Self {
            id: config.id.unwrap_or(0),
            name: config.name,
            provider_type: config.provider_type,
            config: config.config,
            refresh_interval: config.refresh_interval,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PermissionCheckResult {
    pub permission_status: Option<PermissionStatus>,
    pub features: Vec<FeatureAvailability>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRefreshIntervalRequest {
    pub refresh_interval: i64,
}

#[derive(Debug, Deserialize)]
pub struct FetchOrganizationsRequest {
    pub provider_type: String,
    pub config: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct ValidateCredentialsRequest {
    pub provider_type: String,
    pub config: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct CheckPermissionsRequest {
    pub provider_type: String,
    pub config: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct PreviewPipelinesRequest {
    pub provider_type: String,
    pub config: HashMap<String, String>,
    pub org: Option<String>,
    pub search: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct ImportProvidersRequest {
    pub content: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_providers))
        .route("/", post(create_provider))
        .route("/export", get(export_providers))
        .route("/import", post(import_providers))
        .route("/{id}", get(get_provider))
        .route("/{id}", put(update_provider))
        .route("/{id}", delete(remove_provider))
        .route("/{id}/validate", post(validate_existing_credentials))
        .route("/{id}/refresh", post(refresh_provider))
        .route("/{id}/refresh-interval", put(update_refresh_interval))
        .route("/{id}/organizations", get(get_provider_organizations))
        .route("/{id}/permissions", get(get_provider_permissions))
        .route("/{id}/features", get(get_provider_features))
        .route("/{id}/table-schema", get(get_provider_table_schema))
        .route("/validate", post(validate_credentials))
        .route("/organizations", post(fetch_organizations))
        .route("/permissions/check", post(check_permissions))
        .route("/preview", post(preview_pipelines))
        .route("/field-options", post(get_field_options))
}

async fn refresh_provider(
    State(state): State<AppState>, Path(id): Path<i64>,
) -> ApiResult<Json<RefreshResult>> {
    let pipeline_service = state.pipeline_service().await?;
    Ok(Json(pipeline_service.refresh_provider(id).await))
}

async fn export_providers(State(state): State<AppState>) -> ApiResult<Json<String>> {
    let provider_service = state.provider_service().await?;
    let toml = provider_service.export_providers_toml().await?;

    Ok(Json(toml))
}

async fn import_providers(
    State(state): State<AppState>, Json(req): Json<ImportProvidersRequest>,
) -> ApiResult<Json<ProviderImportResult>> {
    let provider_service = state.provider_service().await?;
    let result = provider_service.import_providers_toml(&req.content).await?;

    Ok(Json(result))
}

async fn list_providers(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<pipedash_core::ProviderSummary>>> {
    let provider_service = state.provider_service().await?;
    let providers = provider_service.list_providers().await?;

    Ok(Json(providers))
}

async fn create_provider(
    State(state): State<AppState>, Json(req): Json<CreateProviderRequest>,
) -> ApiResult<Json<ProviderResponse>> {
    let config = ProviderConfig {
        id: None,
        name: req.name,
        provider_type: req.provider_type.clone(),
        token: req.token,
        config: req.config,
        refresh_interval: req.refresh_interval,
        version: None,
    };

    let provider_service = state.provider_service().await?;
    let pipeline_service = state.pipeline_service().await?;
    let id = provider_service.add_provider(config.clone()).await?;

    tracing::info!(
        provider_id = id,
        provider_type = %config.provider_type,
        "Provider created successfully, token persistence in background"
    );

    let results = pipeline_service
        .fetch_now(&[id], PipelineService::REFRESH_DEADLINE)
        .await;
    if let Some(result) = results
        .iter()
        .find(|result| result.state == RefreshState::Failed)
    {
        tracing::warn!(
            provider_id = id,
            kind = ?result.kind,
            message = ?result.message,
            "Initial fetch for new provider failed"
        );
    }

    Ok(Json(ProviderResponse {
        id,
        name: config.name,
        provider_type: config.provider_type,
        config: config.config,
        refresh_interval: config.refresh_interval,
    }))
}

async fn get_provider(
    State(state): State<AppState>, Path(id): Path<i64>,
) -> ApiResult<Json<ProviderResponse>> {
    let provider_service = state.provider_service().await?;
    let provider = provider_service.get_provider_config(id).await?;
    Ok(Json(provider.into()))
}

async fn update_provider(
    State(state): State<AppState>, Path(id): Path<i64>, Json(req): Json<UpdateProviderRequest>,
) -> ApiResult<Json<ProviderResponse>> {
    let provider_service = state.provider_service().await?;
    let existing = provider_service.get_provider_config(id).await?;

    let config = ProviderConfig {
        id: Some(id),
        name: req.name,
        provider_type: existing.provider_type.clone(),
        token: req.token,
        config: req.config,
        refresh_interval: req.refresh_interval,
        version: existing.version,
    };

    provider_service.update_provider(id, config.clone()).await?;

    Ok(Json(ProviderResponse {
        id,
        name: config.name,
        provider_type: existing.provider_type,
        config: config.config,
        refresh_interval: config.refresh_interval,
    }))
}

async fn remove_provider(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<()> {
    let provider_service = state.provider_service().await?;
    provider_service.remove_provider(id).await?;
    Ok(())
}

async fn validate_existing_credentials(
    State(state): State<AppState>, Path(id): Path<i64>,
) -> ApiResult<Json<bool>> {
    let provider_service = state.provider_service().await?;
    let provider = provider_service.get_provider_instance(id).await?;
    let valid = provider.validate_credentials().await?;
    Ok(Json(valid))
}

async fn update_refresh_interval(
    State(state): State<AppState>, Path(id): Path<i64>,
    Json(req): Json<UpdateRefreshIntervalRequest>,
) -> ApiResult<()> {
    if req.refresh_interval < 5 {
        return Err(AppError::bad_request(
            "Refresh interval must be at least 5 seconds",
        ));
    }

    let provider_service = state.provider_service().await?;
    provider_service
        .update_provider_refresh_interval(id, req.refresh_interval)
        .await?;
    Ok(())
}

async fn get_provider_organizations(
    State(state): State<AppState>, Path(id): Path<i64>,
) -> ApiResult<Json<Vec<Organization>>> {
    let provider_service = state.provider_service().await?;
    let config = provider_service.get_provider_config(id).await?;

    let mut plugin_config = config.config.clone();
    plugin_config.insert("token".to_string(), config.token.clone());

    let plugin = provider_service.init_plugin(&config.provider_type, plugin_config, id)?;

    let orgs = plugin
        .fetch_organizations()
        .await
        .map_err(|e| AppError::internal(format!("Failed to fetch organizations: {e}")))?;
    Ok(Json(orgs))
}

async fn get_provider_permissions(
    State(state): State<AppState>, Path(id): Path<i64>,
) -> ApiResult<Json<Option<PermissionStatus>>> {
    let provider_service = state.provider_service().await?;
    let permissions = provider_service.get_provider_permissions(id).await?;
    Ok(Json(permissions))
}

async fn get_provider_features(
    State(state): State<AppState>, Path(id): Path<i64>,
) -> ApiResult<Json<Vec<FeatureAvailability>>> {
    let provider_service = state.provider_service().await?;
    let provider_config = provider_service.get_provider_config(id).await?;

    let metadata_list = provider_service.list_available_plugins();
    let plugin_metadata = metadata_list
        .iter()
        .find(|m| m.provider_type == provider_config.provider_type)
        .ok_or_else(|| {
            AppError::not_found(format!(
                "Plugin not found for provider type: {}",
                provider_config.provider_type
            ))
        })?;

    let permission_status = provider_service.get_provider_permissions(id).await?;

    let permission_status = match permission_status {
        Some(status) => status,
        None => {
            return Ok(Json(
                plugin_metadata
                    .features
                    .iter()
                    .map(|f| FeatureAvailability {
                        feature: f.clone(),
                        available: false,
                        missing_permissions: f.required_permissions.clone(),
                    })
                    .collect(),
            ));
        }
    };

    let granted_perms: std::collections::HashSet<String> = permission_status
        .permissions
        .iter()
        .filter(|p| p.granted)
        .map(|p| p.permission.name.clone())
        .collect();

    let features = plugin_metadata
        .features
        .iter()
        .map(|feature| {
            let missing: Vec<String> = feature
                .required_permissions
                .iter()
                .filter(|p| !granted_perms.contains(*p))
                .cloned()
                .collect();

            FeatureAvailability {
                feature: feature.clone(),
                available: missing.is_empty(),
                missing_permissions: missing,
            }
        })
        .collect();

    Ok(Json(features))
}

async fn get_provider_table_schema(
    State(state): State<AppState>, Path(id): Path<i64>,
) -> ApiResult<Json<pipedash_plugin_api::schema::TableSchema>> {
    let provider_service = state.provider_service().await?;
    let provider_config = provider_service.get_provider_config(id).await?;

    let metadata_list = provider_service.list_available_plugins();
    let plugin_metadata = metadata_list
        .iter()
        .find(|m| m.provider_type == provider_config.provider_type)
        .ok_or_else(|| {
            AppError::not_found(format!(
                "Plugin not found for provider type: {}",
                provider_config.provider_type
            ))
        })?;

    Ok(Json(plugin_metadata.table_schema.clone()))
}

async fn validate_credentials(
    State(state): State<AppState>, Json(req): Json<ValidateCredentialsRequest>,
) -> ApiResult<Json<ValidationResult>> {
    let plugin = state
        .provider_service()
        .await?
        .init_plugin(&req.provider_type, req.config, 0)?;

    match plugin.validate_credentials().await {
        Ok(valid) if valid => Ok(Json(ValidationResult {
            valid: true,
            error: None,
        })),
        Ok(_) => Ok(Json(ValidationResult {
            valid: false,
            error: Some("Invalid credentials".to_string()),
        })),
        Err(e) => Ok(Json(ValidationResult {
            valid: false,
            error: Some(e.to_string()),
        })),
    }
}

async fn fetch_organizations(
    State(state): State<AppState>, Json(req): Json<FetchOrganizationsRequest>,
) -> ApiResult<Json<Vec<Organization>>> {
    let plugin = state
        .provider_service()
        .await?
        .init_plugin(&req.provider_type, req.config, 0)?;

    plugin
        .validate_credentials()
        .await
        .map_err(|e| AppError::internal(format!("Failed to validate credentials: {e}")))?;

    let orgs = plugin
        .fetch_organizations()
        .await
        .map_err(|e| AppError::internal(format!("Failed to fetch organizations: {e}")))?;

    Ok(Json(orgs))
}

async fn check_permissions(
    State(state): State<AppState>, Json(req): Json<CheckPermissionsRequest>,
) -> ApiResult<Json<PermissionCheckResult>> {
    let plugin = state
        .provider_service()
        .await?
        .init_plugin(&req.provider_type, req.config, 0)?;

    let permission_status = plugin.check_permissions().await.ok();

    let features = if let Some(status) = &permission_status {
        plugin.get_feature_availability(status)
    } else {
        Vec::new()
    };

    Ok(Json(PermissionCheckResult {
        permission_status,
        features,
    }))
}

async fn preview_pipelines(
    State(state): State<AppState>, Json(req): Json<PreviewPipelinesRequest>,
) -> ApiResult<Json<PaginatedAvailablePipelines>> {
    let plugin = state
        .provider_service()
        .await?
        .init_plugin(&req.provider_type, req.config, 0)?;

    plugin
        .validate_credentials()
        .await
        .map_err(|e| AppError::internal(format!("Failed to validate credentials: {e}")))?;

    let params = PaginationParams {
        page: req.page.unwrap_or(1).max(1),
        page_size: req.page_size.unwrap_or(100).clamp(10, 200),
    };

    if let Err(validation_error) = params.validate() {
        return Err(AppError::bad_request(validation_error));
    }

    let result = plugin
        .fetch_available_pipelines_filtered(req.org, req.search, Some(params))
        .await
        .map_err(|e| AppError::internal(format!("Failed to fetch available pipelines: {e}")))?;

    Ok(Json(result))
}

#[derive(Debug, Deserialize)]
pub struct GetFieldOptionsRequest {
    pub provider_type: String,
    pub field_key: String,
    #[serde(default)]
    pub config: HashMap<String, String>,
}

async fn get_field_options(
    State(state): State<AppState>, Json(req): Json<GetFieldOptionsRequest>,
) -> ApiResult<Json<Vec<String>>> {
    let provider_service = state.provider_service().await?;
    let options = provider_service
        .field_options(&req.provider_type, &req.field_key, req.config)
        .await
        .map_err(|e| AppError::internal(format!("Failed to fetch field options: {e}")))?;

    Ok(Json(options))
}
