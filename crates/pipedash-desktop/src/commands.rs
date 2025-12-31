use std::collections::HashMap;
use std::sync::Arc;

use pipedash_core::{
    application::RefreshMode,
    domain::{
        AggregatedMetrics,
        AggregationPeriod,
        AggregationType,
        GlobalMetricsConfig,
        MetricType,
        MetricsConfig,
        MetricsQuery,
        MetricsStats,
        PaginatedAvailablePipelines,
        PaginatedRunHistory,
        PaginationParams,
        Pipeline,
        PipelineRun,
        ProviderConfig,
        ProviderSummary,
        TriggerParams,
    },
    CoreContext,
};
use serde::{
    Deserialize,
    Serialize,
};
use tauri::State;

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl From<pipedash_core::DomainError> for ErrorResponse {
    fn from(err: pipedash_core::DomainError) -> Self {
        ErrorResponse {
            error: err.to_string(),
            details: None,
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
    pub permission_status: Option<pipedash_plugin_api::PermissionStatus>,
    pub features: Vec<pipedash_plugin_api::FeatureAvailability>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheStats {
    pub pipelines_count: i64,
    pub run_history_count: i64,
    pub workflow_params_count: i64,
    pub metrics_count: i64,
}

#[tauri::command]
pub async fn add_provider(
    maybe_core: State<'_, crate::MaybeCoreContext>, config: ProviderConfig,
) -> Result<i64, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let id = core.provider_service.add_provider(config).await?;

    if let Err(e) = core.pipeline_service.fetch_pipelines(Some(id)).await {
        tracing::warn!(
            provider_id = id,
            error = %e,
            "Failed to fetch initial pipelines for provider (will retry in background)"
        );
    }

    core.refresh_manager.prioritize_provider(id).await;

    Ok(id)
}

#[tauri::command]
pub async fn list_providers(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<Vec<ProviderSummary>, ErrorResponse> {
    tracing::debug!("[list_providers] Command invoked");
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let result = core.provider_service.list_providers().await;
    match &result {
        Ok(providers) => {
            tracing::debug!("[list_providers] Returning {} providers", providers.len())
        }
        Err(e) => tracing::error!("[list_providers] Error: {:?}", e),
    }
    result.map_err(Into::into)
}

#[tauri::command]
pub async fn get_provider(
    maybe_core: State<'_, crate::MaybeCoreContext>, id: i64,
) -> Result<ProviderConfig, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.provider_service
        .get_provider_config(id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn update_provider(
    maybe_core: State<'_, crate::MaybeCoreContext>, id: i64, config: ProviderConfig,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.provider_service
        .update_provider(id, config)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn update_provider_refresh_interval(
    maybe_core: State<'_, crate::MaybeCoreContext>, id: i64, refresh_interval: i64,
) -> Result<(), ErrorResponse> {
    if refresh_interval < 5 {
        return Err(ErrorResponse {
            error: "Refresh interval must be at least 5 seconds".to_string(),
            details: None,
        });
    }

    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.provider_service
        .update_provider_refresh_interval(id, refresh_interval)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn remove_provider(
    maybe_core: State<'_, crate::MaybeCoreContext>, id: i64,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.provider_service
        .remove_provider(id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_available_plugins(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<Vec<pipedash_plugin_api::PluginMetadata>, ErrorResponse> {
    let core_guard = maybe_core.0.read().await;
    if let Some(core) = core_guard.as_ref() {
        Ok(core.provider_service.list_available_plugins())
    } else {
        use pipedash_core::plugins::create_plugin_registry;
        let registry = create_plugin_registry();
        let mut metadata_list = Vec::new();
        for provider_type in registry.provider_types() {
            if let Some(plugin) = registry.get(&provider_type) {
                metadata_list.push(plugin.metadata().clone());
            }
        }
        Ok(metadata_list)
    }
}

#[tauri::command]
pub async fn list_plugin_metadata(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<Vec<pipedash_plugin_api::PluginMetadata>, ErrorResponse> {
    let core_guard = maybe_core.0.read().await;
    if let Some(core) = core_guard.as_ref() {
        Ok(core.provider_service.list_available_plugins())
    } else {
        use pipedash_core::plugins::create_plugin_registry;
        let registry = create_plugin_registry();
        let mut metadata_list = Vec::new();
        for provider_type in registry.provider_types() {
            if let Some(plugin) = registry.get(&provider_type) {
                metadata_list.push(plugin.metadata().clone());
            }
        }
        Ok(metadata_list)
    }
}

#[tauri::command]
pub async fn get_provider_field_options(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_type: String, field_key: String,
    config: HashMap<String, String>,
) -> Result<Vec<String>, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let plugin = core
        .provider_service
        .create_uninitialized_plugin(&provider_type)?;

    plugin
        .get_field_options(&field_key, &config)
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to fetch field options: {e}"),
            details: None,
        })
}

#[tauri::command]
pub async fn fetch_provider_organizations(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_type: String,
    config: HashMap<String, String>,
) -> Result<Vec<pipedash_plugin_api::Organization>, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let mut plugin = core
        .provider_service
        .create_uninitialized_plugin(&provider_type)?;

    plugin
        .initialize(0, config, None)
        .map_err(|e| ErrorResponse {
            error: format!("Failed to initialize plugin: {e}"),
            details: None,
        })?;

    plugin
        .validate_credentials()
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to validate credentials: {e}"),
            details: None,
        })?;

    plugin
        .fetch_organizations()
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to fetch organizations: {e}"),
            details: None,
        })
}

#[tauri::command]
pub async fn preview_provider_pipelines(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_type: String,
    config: HashMap<String, String>, org: Option<String>, search: Option<String>,
    page: Option<usize>, page_size: Option<usize>,
) -> Result<PaginatedAvailablePipelines, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let mut plugin = core
        .provider_service
        .create_uninitialized_plugin(&provider_type)?;

    plugin
        .initialize(0, config, None)
        .map_err(|e| ErrorResponse {
            error: format!("Failed to initialize plugin: {e}"),
            details: None,
        })?;

    plugin
        .validate_credentials()
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to validate credentials: {e}"),
            details: None,
        })?;

    let params = PaginationParams {
        page: page.unwrap_or(1).max(1),
        page_size: page_size.unwrap_or(100).clamp(10, 200),
    };

    if let Err(validation_error) = params.validate() {
        return Err(ErrorResponse {
            error: validation_error,
            details: None,
        });
    }

    plugin
        .fetch_available_pipelines_filtered(org, search, Some(params))
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to fetch available pipelines: {e}"),
            details: None,
        })
}

#[tauri::command]
pub async fn validate_provider_credentials(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_type: String,
    config: HashMap<String, String>,
) -> Result<ValidationResult, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let mut plugin = core
        .provider_service
        .create_uninitialized_plugin(&provider_type)?;

    plugin
        .initialize(0, config, None)
        .map_err(|e| ErrorResponse {
            error: format!("Failed to initialize plugin: {}", e),
            details: None,
        })?;

    match plugin.validate_credentials().await {
        Ok(valid) if valid => Ok(ValidationResult {
            valid: true,
            error: None,
        }),
        Ok(_) => Ok(ValidationResult {
            valid: false,
            error: Some("Invalid credentials".to_string()),
        }),
        Err(e) => Ok(ValidationResult {
            valid: false,
            error: Some(e.to_string()),
        }),
    }
}

#[tauri::command]
pub async fn check_provider_permissions(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_type: String,
    config: HashMap<String, String>,
) -> Result<PermissionCheckResult, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let mut plugin = core
        .provider_service
        .create_uninitialized_plugin(&provider_type)?;

    plugin
        .initialize(0, config, None)
        .map_err(|e| ErrorResponse {
            error: format!("Failed to initialize plugin: {}", e),
            details: None,
        })?;

    let permission_status = plugin.check_permissions().await.ok();

    let features = if let Some(status) = &permission_status {
        plugin.get_feature_availability(status)
    } else {
        Vec::new()
    };

    Ok(PermissionCheckResult {
        permission_status,
        features,
    })
}

#[tauri::command]
pub async fn get_provider_permissions(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_id: i64,
) -> Result<Option<pipedash_plugin_api::PermissionStatus>, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.provider_service
        .get_provider_permissions(provider_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_provider_features(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_id: i64,
) -> Result<Vec<pipedash_plugin_api::FeatureAvailability>, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let provider_config = core
        .provider_service
        .get_provider_config(provider_id)
        .await?;

    let metadata_list = core.provider_service.list_available_plugins();
    let plugin_metadata = metadata_list
        .iter()
        .find(|m| m.provider_type == provider_config.provider_type)
        .ok_or_else(|| ErrorResponse {
            error: format!(
                "Plugin not found for provider type: {}",
                provider_config.provider_type
            ),
            details: None,
        })?;

    let permission_status = core
        .provider_service
        .get_provider_permissions(provider_id)
        .await?;

    let permission_status = match permission_status {
        Some(status) => status,
        None => {
            return Ok(plugin_metadata
                .features
                .iter()
                .map(|f| pipedash_plugin_api::FeatureAvailability {
                    feature: f.clone(),
                    available: false,
                    missing_permissions: f.required_permissions.clone(),
                })
                .collect());
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

            pipedash_plugin_api::FeatureAvailability {
                feature: feature.clone(),
                available: missing.is_empty(),
                missing_permissions: missing,
            }
        })
        .collect();

    Ok(features)
}

#[tauri::command]
pub async fn get_provider_table_schema(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_id: i64,
) -> Result<pipedash_plugin_api::schema::TableSchema, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let provider_config = core
        .provider_service
        .get_provider_config(provider_id)
        .await?;

    let metadata_list = core.provider_service.list_available_plugins();
    let plugin_metadata = metadata_list
        .iter()
        .find(|m| m.provider_type == provider_config.provider_type)
        .ok_or_else(|| ErrorResponse {
            error: format!(
                "Plugin not found for provider type: {}",
                provider_config.provider_type
            ),
            details: None,
        })?;

    Ok(plugin_metadata.table_schema.clone())
}

#[tauri::command]
pub async fn fetch_pipelines(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_id: Option<i64>,
) -> Result<Vec<Pipeline>, ErrorResponse> {
    tracing::debug!(
        "[fetch_pipelines] Command invoked with provider_id: {:?}",
        provider_id
    );
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let result = core.pipeline_service.fetch_pipelines(provider_id).await;
    match &result {
        Ok(pipelines) => {
            tracing::debug!("[fetch_pipelines] Returning {} pipelines", pipelines.len())
        }
        Err(e) => tracing::error!("[fetch_pipelines] Error: {:?}", e),
    }
    result.map_err(Into::into)
}

#[tauri::command]
pub async fn get_cached_pipelines(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_id: Option<i64>,
) -> Result<Vec<Pipeline>, ErrorResponse> {
    tracing::debug!(
        "[get_cached_pipelines] Command invoked with provider_id: {:?}",
        provider_id
    );
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let result = core
        .pipeline_service
        .get_cached_pipelines(provider_id)
        .await;
    match &result {
        Ok(pipelines) => tracing::debug!(
            "[get_cached_pipelines] Returning {} pipelines",
            pipelines.len()
        ),
        Err(e) => tracing::error!("[get_cached_pipelines] Error: {:?}", e),
    }
    result.map_err(Into::into)
}

#[tauri::command]
pub async fn fetch_run_history(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: String, page: Option<usize>,
    page_size: Option<usize>,
) -> Result<PaginatedRunHistory, ErrorResponse> {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).clamp(10, 100);

    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.pipeline_service
        .fetch_run_history_paginated(&pipeline_id, page, page_size)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_workflow_run_details(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: String, run_number: i64,
) -> Result<PipelineRun, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.pipeline_service
        .fetch_run_details(&pipeline_id, run_number)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn trigger_pipeline(
    maybe_core: State<'_, crate::MaybeCoreContext>, params: TriggerParams,
) -> Result<String, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let workflow_id = params.workflow_id.clone();
    let result = core.pipeline_service.trigger_pipeline(params).await?;

    core.pipeline_service
        .invalidate_run_cache(&workflow_id)
        .await;

    Ok(result)
}

#[tauri::command]
pub async fn cancel_pipeline_run(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: String, run_number: i64,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.pipeline_service
        .cancel_run(&pipeline_id, run_number)
        .await?;
    core.pipeline_service
        .invalidate_run_cache(&pipeline_id)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn get_workflow_parameters(
    maybe_core: State<'_, crate::MaybeCoreContext>, workflow_id: String,
) -> Result<Vec<pipedash_plugin_api::WorkflowParameter>, ErrorResponse> {
    let parts: Vec<&str> = workflow_id.split("__").collect();
    if parts.len() < 2 {
        return Err(ErrorResponse {
            error: format!(
                "Invalid workflow ID format '{}'. Expected format: 'provider__id__...'",
                workflow_id
            ),
            details: None,
        });
    }

    let provider_id: i64 = parts[1].parse().map_err(|_| ErrorResponse {
        error: format!(
            "Invalid provider ID '{}' in workflow ID. Expected numeric ID.",
            parts[1]
        ),
        details: None,
    })?;

    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.provider_service
        .get_workflow_parameters(provider_id, &workflow_id)
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to fetch workflow parameters: {e}"),
            details: None,
        })
}

#[tauri::command]
pub async fn refresh_all(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.pipeline_service.clear_all_run_history_caches().await;
    core.pipeline_service
        .refresh_all()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn set_refresh_mode(
    maybe_core: State<'_, crate::MaybeCoreContext>, mode: String,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let refresh_mode = mode.parse::<RefreshMode>().unwrap_or(RefreshMode::Idle);
    core.refresh_manager.set_mode(refresh_mode).await;
    Ok(())
}

#[tauri::command]
pub async fn get_refresh_mode(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<String, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let mode = core.refresh_manager.get_mode().await;
    Ok(mode.as_str().to_string())
}

#[tauri::command]
pub async fn clear_run_history_cache(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: String,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.pipeline_service
        .clear_run_history_cache(&pipeline_id)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn get_cache_stats(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<CacheStats, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let pipelines_count = core
        .pipeline_service
        .get_pipelines_cache_count()
        .await
        .unwrap_or(0);
    let run_history_count = core
        .pipeline_service
        .get_run_history_cache_count()
        .await
        .unwrap_or(0);
    let workflow_params_count = core
        .pipeline_service
        .get_workflow_params_cache_count()
        .await
        .unwrap_or(0);

    let metrics_count = if let Some(metrics_service) = &core.metrics_service {
        metrics_service
            .get_storage_stats()
            .await
            .map(|stats| stats.total_metrics_count)
            .unwrap_or(0)
    } else {
        0
    };

    Ok(CacheStats {
        pipelines_count,
        run_history_count,
        workflow_params_count,
        metrics_count,
    })
}

#[tauri::command]
pub async fn clear_pipelines_cache(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<usize, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.pipeline_service
        .clear_pipelines_cache()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn clear_all_run_history_caches(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.pipeline_service.clear_all_run_history_caches().await;
    Ok(())
}

#[tauri::command]
pub async fn clear_workflow_params_cache(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.pipeline_service
        .clear_workflow_params_cache()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn clear_all_caches(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.pipeline_service
        .clear_all_caches_atomic()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_global_metrics_config(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<GlobalMetricsConfig, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let metrics_service = core.metrics_service.as_ref().ok_or_else(|| ErrorResponse {
        error: "Metrics service not available".to_string(),
        details: None,
    })?;

    metrics_service
        .get_global_config()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn update_global_metrics_config(
    maybe_core: State<'_, crate::MaybeCoreContext>, enabled: bool, default_retention_days: i64,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let metrics_service = core.metrics_service.as_ref().ok_or_else(|| ErrorResponse {
        error: "Metrics service not available".to_string(),
        details: None,
    })?;

    metrics_service
        .update_global_config(enabled, default_retention_days)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_pipeline_metrics_config(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: String,
) -> Result<MetricsConfig, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let metrics_service = core.metrics_service.as_ref().ok_or_else(|| ErrorResponse {
        error: "Metrics service not available".to_string(),
        details: None,
    })?;

    metrics_service
        .get_effective_pipeline_config(&pipeline_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn update_pipeline_metrics_config(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: String, enabled: bool,
    retention_days: i64,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let metrics_service = core.metrics_service.as_ref().ok_or_else(|| ErrorResponse {
        error: "Metrics service not available".to_string(),
        details: None,
    })?;

    metrics_service
        .update_pipeline_config(&pipeline_id, enabled, retention_days)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn query_pipeline_metrics(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: Option<String>,
    metric_type: Option<String>, start_date: Option<String>, end_date: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<pipedash_core::domain::MetricEntry>, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let metrics_service = core.metrics_service.as_ref().ok_or_else(|| ErrorResponse {
        error: "Metrics service not available".to_string(),
        details: None,
    })?;

    let parsed_metric_type = metric_type.and_then(|t| t.parse::<MetricType>().ok());
    let parsed_start_date = start_date
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));
    let parsed_end_date = end_date
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let query = MetricsQuery {
        pipeline_id,
        metric_type: parsed_metric_type,
        start_date: parsed_start_date,
        end_date: parsed_end_date,
        aggregation_period: None,
        aggregation_type: None,
        limit,
    };

    metrics_service
        .query_metrics(query)
        .await
        .map_err(Into::into)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn query_aggregated_metrics(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: Option<String>,
    metric_type: String, aggregation_period: String, aggregation_type: Option<String>,
    start_date: Option<String>, end_date: Option<String>, limit: Option<usize>,
) -> Result<AggregatedMetrics, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let metrics_service = core.metrics_service.as_ref().ok_or_else(|| ErrorResponse {
        error: "Metrics service not available".to_string(),
        details: None,
    })?;

    let parsed_metric_type = metric_type
        .parse::<MetricType>()
        .map_err(|_| ErrorResponse {
            error: format!("Invalid metric type: {}", metric_type),
            details: None,
        })?;

    let parsed_aggregation = match aggregation_period.as_str() {
        "hourly" => AggregationPeriod::Hourly,
        "daily" => AggregationPeriod::Daily,
        "weekly" => AggregationPeriod::Weekly,
        "monthly" => AggregationPeriod::Monthly,
        _ => {
            return Err(ErrorResponse {
                error: format!("Invalid aggregation period: {}", aggregation_period),
                details: None,
            })
        }
    };

    let parsed_aggregation_type = aggregation_type
        .as_ref()
        .map(|s| match s.as_str() {
            "avg" => Ok(AggregationType::Avg),
            "sum" => Ok(AggregationType::Sum),
            "min" => Ok(AggregationType::Min),
            "max" => Ok(AggregationType::Max),
            "p95" => Ok(AggregationType::P95),
            "p99" => Ok(AggregationType::P99),
            _ => Err(ErrorResponse {
                error: format!("Invalid aggregation type: {}", s),
                details: None,
            }),
        })
        .transpose()?;

    let parsed_start_date = start_date
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));
    let parsed_end_date = end_date
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let query = MetricsQuery {
        pipeline_id,
        metric_type: Some(parsed_metric_type),
        start_date: parsed_start_date,
        end_date: parsed_end_date,
        aggregation_period: Some(parsed_aggregation),
        aggregation_type: parsed_aggregation_type,
        limit,
    };

    metrics_service
        .query_aggregated_metrics(query)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_metrics_storage_stats(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<MetricsStats, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let metrics_service = core.metrics_service.as_ref().ok_or_else(|| ErrorResponse {
        error: "Metrics service not available".to_string(),
        details: None,
    })?;

    metrics_service
        .get_storage_stats()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn flush_pipeline_metrics(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: Option<String>,
) -> Result<usize, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let metrics_service = core.metrics_service.as_ref().ok_or_else(|| ErrorResponse {
        error: "Metrics service not available".to_string(),
        details: None,
    })?;

    metrics_service
        .flush_metrics(pipeline_id.as_deref(), false)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn reset_metrics_processing_state(
    maybe_core: State<'_, crate::MaybeCoreContext>, pipeline_id: String,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let metrics_service = core.metrics_service.as_ref().ok_or_else(|| ErrorResponse {
        error: "Metrics service not available".to_string(),
        details: None,
    })?;

    metrics_service
        .reset_pipeline_processing(&pipeline_id)
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to reset processing state: {}", e),
            details: None,
        })?;

    Ok(())
}

#[tauri::command]
pub async fn get_table_preferences(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_id: i64, table_id: String,
) -> Result<Option<String>, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.provider_service
        .repository()
        .get_table_preferences(provider_id, &table_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn save_table_preferences(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_id: i64, table_id: String,
    preferences_json: String,
) -> Result<(), ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    core.provider_service
        .repository()
        .upsert_table_preferences(provider_id, &table_id, &preferences_json)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_default_table_preferences(
    maybe_core: State<'_, crate::MaybeCoreContext>, provider_id: i64, table_id: String,
) -> Result<String, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let provider_config = core
        .provider_service
        .get_provider_config(provider_id)
        .await?;

    let metadata_list = core.provider_service.list_available_plugins();
    let plugin_metadata = metadata_list
        .iter()
        .find(|m| m.provider_type == provider_config.provider_type)
        .ok_or_else(|| ErrorResponse {
            error: format!(
                "Plugin not found for provider type: {}",
                provider_config.provider_type
            ),
            details: None,
        })?;

    let table = plugin_metadata
        .table_schema
        .get_table(&table_id)
        .ok_or_else(|| ErrorResponse {
            error: format!("Table '{}' not found in schema", table_id),
            details: None,
        })?;

    let column_visibility: std::collections::HashMap<String, bool> = table
        .columns
        .iter()
        .map(|col| (col.id.clone(), col.default_visible))
        .collect();

    let column_order: Vec<String> = table.columns.iter().map(|col| col.id.clone()).collect();

    #[derive(serde::Serialize)]
    struct DefaultPreferences {
        #[serde(rename = "columnOrder")]
        column_order: Vec<String>,
        #[serde(rename = "columnVisibility")]
        column_visibility: std::collections::HashMap<String, bool>,
    }

    let prefs = DefaultPreferences {
        column_order,
        column_visibility,
    };

    serde_json::to_string(&prefs).map_err(|e| ErrorResponse {
        error: format!("Failed to serialize default preferences: {}", e),
        details: None,
    })
}

use pipedash_core::infrastructure::{
    ConfigLoader,
    MigrationOptions,
    MigrationOrchestrator,
    MigrationPlan,
    MigrationResult,
    PipedashConfig,
    SetupStatus,
    StorageConfig,
    StorageManager,
    ValidationReport,
};

use crate::AppDataDir;

#[tauri::command]
pub async fn check_setup_status(
    app_data_dir: State<'_, AppDataDir>,
) -> Result<SetupStatus, ErrorResponse> {
    Ok(ConfigLoader::get_setup_status(&app_data_dir.0))
}

#[tauri::command]
pub async fn bootstrap_app(
    app: tauri::AppHandle, app_data_dir: State<'_, AppDataDir>,
    maybe_core_context: State<'_, crate::MaybeCoreContext>,
) -> Result<(), ErrorResponse> {
    use std::sync::Arc;

    use pipedash_core::infrastructure::{
        config::ConfigLoader,
        StorageManager,
    };
    use tauri::Manager;

    use crate::{
        keyring_store::KeyringTokenStore,
        tauri_event_bus::create_tauri_event_bus,
    };

    tracing::info!("Bootstrapping application after initial setup");

    let config_path = app_data_dir.0.join("config.toml");

    let config = ConfigLoader::load(&config_path).map_err(|e| ErrorResponse {
        error: format!("Failed to load configuration: {}", e),
        details: None,
    })?;

    let event_bus = create_tauri_event_bus(app.clone());

    let use_keyring = config.storage.backend.is_sqlite();
    let storage_manager = if use_keyring {
        let token_store = Arc::new(KeyringTokenStore::new());
        StorageManager::with_token_store(config.clone(), token_store, true)
            .await
            .map_err(|e| ErrorResponse {
                error: format!("Failed to initialize StorageManager: {}", e),
                details: None,
            })?
    } else {
        StorageManager::from_config_allow_locked(config.clone(), true)
            .await
            .map_err(|e| ErrorResponse {
                error: format!("Failed to initialize StorageManager: {}", e),
                details: None,
            })?
    };

    let core_context =
        pipedash_core::CoreContext::with_storage_manager(&storage_manager, event_bus.clone())
            .await
            .map_err(|e| ErrorResponse {
                error: format!("Failed to create CoreContext: {}", e),
                details: None,
            })?;

    let ctx_arc = Arc::new(core_context);

    {
        let mut guard = maybe_core_context.0.write().await;
        *guard = Some(Arc::clone(&ctx_arc));
    }

    app.manage(Arc::clone(&ctx_arc));

    let core_clone = Arc::clone(&ctx_arc);
    tauri::async_runtime::spawn(async move {
        if let Err(e) = core_clone.warmup_token_store().await {
            tracing::warn!("Token store warmup failed during bootstrap: {}", e);
        }
        core_clone.start_background_tasks().await;
    });

    tracing::info!("Application bootstrap completed successfully");

    Ok(())
}

#[tauri::command]
pub async fn create_initial_config(
    app_data_dir: State<'_, AppDataDir>, config: PipedashConfig, vault_password: Option<String>,
) -> Result<(), ErrorResponse> {
    let config_path = app_data_dir.0.join("config.toml");

    if let Some(password) = &vault_password {
        std::env::set_var("PIPEDASH_VAULT_PASSWORD", password);
        tracing::info!("Vault password set for session (from setup wizard)");
    }

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ErrorResponse {
            error: format!("Failed to create config directory: {}", e),
            details: None,
        })?;
    }

    ConfigLoader::save(&config, &config_path).map_err(|e| ErrorResponse {
        error: format!("Failed to save config: {}", e),
        details: None,
    })?;

    tracing::info!("Created initial config at {:?}", config_path);
    Ok(())
}

fn load_config_from_dir(data_dir: &std::path::Path) -> Result<PipedashConfig, ErrorResponse> {
    let config_path = data_dir.join("config.toml");

    if !config_path.exists() {
        return Err(ErrorResponse {
            error: "Configuration file not found. Please complete the setup wizard first."
                .to_string(),
            details: None,
        });
    }

    ConfigLoader::load(&config_path).map_err(|e| ErrorResponse {
        error: format!("Failed to load config: {}", e),
        details: None,
    })
}

#[derive(Debug, Serialize)]
pub struct StorageConfigResponse {
    pub config: PipedashConfig,
    pub summary: String,
}

#[tauri::command]
pub async fn get_storage_config(
    app_data_dir: State<'_, AppDataDir>,
) -> Result<StorageConfigResponse, ErrorResponse> {
    let config = load_config_from_dir(&app_data_dir.0)?;
    let summary = config.storage.summary();
    Ok(StorageConfigResponse { config, summary })
}

#[derive(Debug, Serialize)]
pub struct VaultPasswordStatus {
    pub is_set: bool,
    pub env_var_name: &'static str,
}

#[tauri::command]
pub async fn get_vault_password_status() -> VaultPasswordStatus {
    let is_set = std::env::var("PIPEDASH_VAULT_PASSWORD").is_ok();
    VaultPasswordStatus {
        is_set,
        env_var_name: "PIPEDASH_VAULT_PASSWORD",
    }
}

#[derive(Debug, Serialize)]
pub struct VaultStatusResponse {
    pub is_unlocked: bool,
    pub password_source: String,
    pub backend: String,
    pub requires_password: bool,
    pub is_first_time: bool,
}

#[derive(Debug, Serialize)]
pub struct UnlockVaultResponse {
    pub success: bool,
    pub message: String,
}

#[tauri::command]
pub async fn get_vault_status(
    maybe_core: State<'_, crate::MaybeCoreContext>, app_data_dir: State<'_, AppDataDir>,
) -> Result<VaultStatusResponse, ErrorResponse> {
    let config = load_config_from_dir(&app_data_dir.0)?;

    let has_encrypted_data = if config.storage.backend.is_sqlite() {
        let db_path = config.data_dir().join("pipedash.db");
        pipedash_core::infrastructure::database::has_encrypted_tokens(&db_path).await
    } else {
        false
    };

    let requires_password = if config.storage.backend.is_sqlite() {
        has_encrypted_data
    } else {
        true // Postgres always requires password
    };

    let vault_password_set = std::env::var("PIPEDASH_VAULT_PASSWORD").is_ok();

    let password_source = if !requires_password {
        "keyring" // Using keyring, no password needed
    } else if vault_password_set {
        "env_var"
    } else {
        "none"
    };

    let is_unlocked = !requires_password || vault_password_set;

    let is_first_time = {
        let core_guard = maybe_core.0.read().await;
        if let Some(core) = core_guard.as_ref() {
            core.provider_service
                .list_providers()
                .await
                .map(|p| p.is_empty())
                .unwrap_or(true)
        } else {
            true
        }
    };

    tracing::debug!(
        "Vault status: requires_password={}, is_unlocked={}, has_encrypted_data={}, backend={}",
        requires_password,
        is_unlocked,
        has_encrypted_data,
        config.storage.backend
    );

    Ok(VaultStatusResponse {
        is_unlocked,
        password_source: password_source.to_string(),
        backend: config.storage.backend.to_string(),
        requires_password,
        is_first_time,
    })
}

#[tauri::command]
pub async fn unlock_vault(
    app: tauri::AppHandle, app_data_dir: State<'_, AppDataDir>,
    maybe_core_context: State<'_, crate::MaybeCoreContext>, password: String,
) -> Result<UnlockVaultResponse, ErrorResponse> {
    use std::sync::Arc;

    use pipedash_core::infrastructure::StorageManager;

    use crate::tauri_event_bus::create_tauri_event_bus;

    let config = load_config_from_dir(&app_data_dir.0)?;

    let db_path = config.data_dir().join("pipedash.db");
    let has_encrypted_tokens =
        pipedash_core::infrastructure::database::has_encrypted_tokens(&db_path).await;

    if config.storage.backend.is_sqlite() && !has_encrypted_tokens {
        if !password.is_empty() {
            std::env::set_var("PIPEDASH_VAULT_PASSWORD", &password);
            tracing::info!("Vault password set for future migration (no encrypted tokens yet)");
        }
        return Ok(UnlockVaultResponse {
            success: true,
            message: "Vault password set for migration".to_string(),
        });
    }

    if password.is_empty() {
        return Ok(UnlockVaultResponse {
            success: false,
            message: "Password required to unlock encrypted vault".to_string(),
        });
    }

    std::env::set_var("PIPEDASH_VAULT_PASSWORD", &password);
    tracing::info!("Vault password set in environment for session");

    let config = load_config_from_dir(&app_data_dir.0)?;

    let storage_manager = match StorageManager::from_config(config.clone(), true).await {
        Ok(manager) => manager,
        Err(e) => {
            std::env::remove_var("PIPEDASH_VAULT_PASSWORD");
            tracing::warn!("Vault unlock failed - invalid password: {}", e);
            return Ok(UnlockVaultResponse {
                success: false,
                message: format!("Invalid password: {}", e),
            });
        }
    };

    match storage_manager.token_store().await.get_all_tokens().await {
        Ok(tokens) => {
            tracing::info!(
                "Password validated - decrypted {} tokens successfully",
                tokens.len()
            );
        }
        Err(e) => {
            std::env::remove_var("PIPEDASH_VAULT_PASSWORD");
            tracing::warn!("Vault unlock failed - token decryption error: {}", e);
            return Ok(UnlockVaultResponse {
                success: false,
                message: format!("Invalid password: failed to decrypt tokens: {}", e),
            });
        }
    }

    let event_bus = create_tauri_event_bus(app.clone());

    let core_context = match CoreContext::with_storage_manager(&storage_manager, event_bus).await {
        Ok(ctx) => Arc::new(ctx),
        Err(e) => {
            tracing::error!("Failed to create CoreContext after vault unlock: {}", e);
            return Ok(UnlockVaultResponse {
                success: false,
                message: format!("Failed to initialize after unlock: {}", e),
            });
        }
    };

    {
        let mut guard = maybe_core_context.0.write().await;
        *guard = Some(Arc::clone(&core_context));
    }

    let core_clone = Arc::clone(&core_context);
    tauri::async_runtime::spawn(async move {
        if let Err(e) = core_clone.warmup_token_store().await {
            tracing::warn!(
                "Token store warmup warning after unlock: {}. Providers may need manual refresh.",
                e
            );
        }

        if let Err(e) = core_clone.provider_service.load_all_providers().await {
            tracing::warn!("Failed to load providers after unlock: {}", e);
        }

        core_clone.start_background_tasks().await;

        core_clone
            .event_bus
            .emit(pipedash_core::event::CoreEvent::VaultUnlocked)
            .await;

        tracing::info!("Vault unlock complete - providers loaded and background tasks started");
    });

    tracing::info!(
        "Vault unlocked successfully - CoreContext recreated with encrypted token store"
    );
    Ok(UnlockVaultResponse {
        success: true,
        message: "Vault unlocked successfully".to_string(),
    })
}

#[tauri::command]
pub async fn lock_vault() -> Result<UnlockVaultResponse, ErrorResponse> {
    std::env::remove_var("PIPEDASH_VAULT_PASSWORD");
    tracing::info!("Vault locked - session password cleared");

    Ok(UnlockVaultResponse {
        success: true,
        message: "Vault locked. Restart required to fully clear token cache.".to_string(),
    })
}

#[tauri::command]
pub async fn save_storage_config(
    _core: State<'_, Arc<CoreContext>>, app_data_dir: State<'_, AppDataDir>,
    config: PipedashConfig, token_password: Option<String>,
) -> Result<(), ErrorResponse> {
    if let Some(password) = &token_password {
        std::env::set_var("PIPEDASH_VAULT_PASSWORD", password);
        tracing::info!("Vault password set for storage migration");
    }

    let config_path = app_data_dir.0.join("config.toml");

    tracing::info!(
        "save_storage_config: path={}, data_dir={:?}, backend={}",
        config_path.display(),
        config.data_dir(),
        config.storage.backend
    );

    ConfigLoader::save(&config, &config_path).map_err(|e| ErrorResponse {
        error: format!("Failed to save storage config: {}", e),
        details: None,
    })?;

    tracing::info!("Storage config saved successfully");
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct ConfigContentResponse {
    pub content: String,
    pub path: String,
}

#[tauri::command]
pub async fn get_config_content(
    app_data_dir: State<'_, AppDataDir>,
) -> Result<ConfigContentResponse, ErrorResponse> {
    let config_path = app_data_dir.0.join("config.toml");

    let content = std::fs::read_to_string(&config_path).map_err(|e| ErrorResponse {
        error: format!("Failed to read config file: {}", e),
        details: None,
    })?;

    Ok(ConfigContentResponse {
        content,
        path: config_path.display().to_string(),
    })
}

#[tauri::command]
pub async fn save_config_content(
    app_data_dir: State<'_, AppDataDir>, content: String,
) -> Result<(), ErrorResponse> {
    let config: PipedashConfig = toml::from_str(&content).map_err(|e| ErrorResponse {
        error: format!("Invalid TOML syntax: {}", e),
        details: None,
    })?;

    let validation = config.validate();
    if !validation.is_ok() {
        let errors: Vec<String> = validation
            .errors
            .iter()
            .map(|e| format!("{:?}", e))
            .collect();
        return Err(ErrorResponse {
            error: format!("Config validation failed: {}", errors.join(", ")),
            details: None,
        });
    }

    let config_path = app_data_dir.0.join("config.toml");
    std::fs::write(&config_path, &content).map_err(|e| ErrorResponse {
        error: format!("Failed to save config file: {}", e),
        details: None,
    })?;

    tracing::info!("Config file saved: {:?}", config_path);
    Ok(())
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

#[tauri::command]
pub async fn get_storage_paths(
    app_data_dir: State<'_, AppDataDir>, _maybe_core_context: State<'_, crate::MaybeCoreContext>,
) -> Result<StoragePathsResponse, ErrorResponse> {
    use pipedash_core::infrastructure::ConfigLoader;

    let config_path = app_data_dir.0.join("config.toml");
    let effective_data_dir = if config_path.exists() {
        match ConfigLoader::load(&config_path) {
            Ok(config) => config.data_dir(),
            Err(_) => app_data_dir.0.clone(), // Fallback to app_data_dir if config can't be loaded
        }
    } else {
        app_data_dir.0.clone() // No config yet, use app_data_dir
    };

    Ok(StoragePathsResponse {
        config_file: app_data_dir.0.join("config.toml").display().to_string(), /* Config always in app_data_dir */
        pipedash_db: effective_data_dir.join("pipedash.db").display().to_string(),
        metrics_db: effective_data_dir.join("metrics.db").display().to_string(),
        data_dir: effective_data_dir.display().to_string(),
        cache_dir: effective_data_dir.join("cache").display().to_string(),
        vault_path: effective_data_dir.join("vault").display().to_string(),
    })
}

#[tauri::command]
pub async fn get_default_data_dir() -> Result<String, ErrorResponse> {
    use pipedash_core::infrastructure::PipedashConfig;

    let default_dir = PipedashConfig::default_data_dir();
    Ok(default_dir.display().to_string())
}

#[tauri::command]
pub async fn get_effective_data_dir(config: PipedashConfig) -> Result<String, ErrorResponse> {
    let effective_dir = config.data_dir();
    Ok(effective_dir.display().to_string())
}

#[derive(Debug, Serialize)]
pub struct DatabaseExistsResponse {
    pub exists: bool,
    pub path: String,
}

#[tauri::command]
pub async fn check_database_exists(
    config: PipedashConfig,
) -> Result<DatabaseExistsResponse, ErrorResponse> {
    let db_path = config.db_path();
    let exists = db_path.exists();

    Ok(DatabaseExistsResponse {
        exists,
        path: db_path.display().to_string(),
    })
}

#[derive(Debug, Serialize)]
pub struct TestConnectionResult {
    pub success: bool,
    pub message: String,
}

#[tauri::command]
#[allow(dead_code)]
pub async fn test_storage_connection(
    maybe_core: State<'_, crate::MaybeCoreContext>, config: PipedashConfig,
) -> Result<TestConnectionResult, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
    })?;
    let manager = StorageManager::with_token_store(config.clone(), core.token_store.clone(), true)
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to initialize storage: {}", e),
            details: None,
        })?;

    let config_test = manager
        .config_backend()
        .list_providers()
        .await
        .map(|_| ())
        .map_err(|e| format!("Config backend: {}", e));

    let cache_test = if manager.cache_backend().is_available().await {
        Ok(())
    } else {
        Err("Cache backend: Storage not available or not accessible".to_string())
    };

    match (config_test, cache_test) {
        (Ok(()), Ok(())) => Ok(TestConnectionResult {
            success: true,
            message: "All storage backends are accessible".to_string(),
        }),
        (Ok(()), Err(cache_err)) => Ok(TestConnectionResult {
            success: false,
            message: format!("Config backend OK, but cache backend failed: {}", cache_err),
        }),
        (Err(config_err), Ok(())) => Ok(TestConnectionResult {
            success: false,
            message: format!(
                "Cache backend OK, but config backend failed: {}",
                config_err
            ),
        }),
        (Err(config_err), Err(cache_err)) => Ok(TestConnectionResult {
            success: false,
            message: format!(
                "Both backends failed - Config: {}, Cache: {}",
                config_err, cache_err
            ),
        }),
    }
}

#[tauri::command]
pub async fn plan_storage_migration(
    maybe_core: State<'_, crate::MaybeCoreContext>, app_data_dir: State<'_, AppDataDir>,
    target_config: PipedashConfig, options: MigrationOptions,
) -> Result<MigrationPlan, ErrorResponse> {
    let current_config = load_config_from_dir(&app_data_dir.0)?;

    let core_guard = maybe_core.0.read().await;
    let core_opt = core_guard.as_ref();

    let manager = if let Some(core) = core_opt {
        StorageManager::with_token_store(current_config.clone(), core.token_store.clone(), true)
            .await
            .map_err(|e| ErrorResponse {
                error: format!("Failed to create storage manager: {}", e),
                details: None,
            })?
    } else {
        StorageManager::from_config_allow_locked(current_config.clone(), true)
            .await
            .map_err(|e| ErrorResponse {
                error: format!("Failed to create storage manager: {}", e),
                details: None,
            })?
    };

    let target_uses_encrypted =
        options.token_password.is_some() || std::env::var("PIPEDASH_VAULT_PASSWORD").is_ok();

    let target_token_store = if let Some(core) = core_opt {
        if target_config.storage.backend.is_sqlite() && !target_uses_encrypted {
            Some(core.token_store.clone())
        } else {
            None
        }
    } else {
        None // Setup mode - no external token store
    };

    tracing::info!(
        "[plan_storage_migration] target_uses_encrypted={}, using_keyring_for_target={}",
        target_uses_encrypted,
        target_token_store.is_some()
    );

    let event_bus = core_opt.map(|c| c.event_bus.clone());

    let orchestrator =
        MigrationOrchestrator::from_manager(&manager, event_bus, target_token_store).await;

    orchestrator
        .plan_migration(target_config, &options)
        .map_err(|e| ErrorResponse {
            error: format!("Failed to plan migration: {}", e),
            details: None,
        })
}

#[tauri::command]
pub async fn execute_storage_migration(
    maybe_core: State<'_, crate::MaybeCoreContext>, _app_data_dir: State<'_, AppDataDir>,
    plan: MigrationPlan, options: MigrationOptions,
) -> Result<MigrationResult, ErrorResponse> {
    let source_config = plan.from.clone();

    let core_guard = maybe_core.0.read().await;
    let core_opt = core_guard.as_ref();

    let manager = if let Some(core) = core_opt {
        StorageManager::with_token_store(source_config, core.token_store.clone(), true)
            .await
            .map_err(|e| ErrorResponse {
                error: format!("Failed to create storage manager: {}", e),
                details: None,
            })?
    } else {
        StorageManager::from_config_allow_locked(source_config, true)
            .await
            .map_err(|e| ErrorResponse {
                error: format!("Failed to create storage manager: {}", e),
                details: None,
            })?
    };

    let target_uses_encrypted =
        options.token_password.is_some() || std::env::var("PIPEDASH_VAULT_PASSWORD").is_ok();

    let target_token_store = if let Some(core) = core_opt {
        if plan.to.storage.backend.is_sqlite() && !target_uses_encrypted {
            Some(core.token_store.clone())
        } else {
            None
        }
    } else {
        None // Setup mode - no external token store
    };

    tracing::info!(
        "[execute_storage_migration] target_uses_encrypted={}, using_keyring_for_target={}",
        target_uses_encrypted,
        target_token_store.is_some()
    );

    let event_bus = core_opt.map(|c| c.event_bus.clone());

    let orchestrator =
        MigrationOrchestrator::from_manager(&manager, event_bus, target_token_store).await;

    Ok(orchestrator.execute_migration(plan, options, true).await)
}

#[tauri::command]
pub async fn validate_storage_config(
    maybe_core: State<'_, crate::MaybeCoreContext>, app_data_dir: State<'_, AppDataDir>,
    config: StorageConfig,
) -> Result<ValidationReport, ErrorResponse> {
    let current_config = load_config_from_dir(&app_data_dir.0)?;

    let core_guard = maybe_core.0.read().await;
    let core_opt = core_guard.as_ref();

    let manager = if let Some(core) = core_opt {
        StorageManager::with_token_store(current_config, core.token_store.clone(), true)
            .await
            .map_err(|e| ErrorResponse {
                error: format!("Failed to create storage manager: {}", e),
                details: None,
            })?
    } else {
        StorageManager::from_config_allow_locked(current_config, true)
            .await
            .map_err(|e| ErrorResponse {
                error: format!("Failed to create storage manager: {}", e),
                details: None,
            })?
    };

    let event_bus = core_opt.map(|c| c.event_bus.clone());
    let token_store = core_opt.map(|c| c.token_store.clone());

    let orchestrator = MigrationOrchestrator::from_manager(&manager, event_bus, token_store).await;

    orchestrator
        .validate_target_config(&config)
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to validate config: {}", e),
            details: None,
        })
}

#[derive(Debug, Serialize)]
pub struct FactoryResetResult {
    pub providers_removed: usize,
    pub caches_cleared: bool,
    pub tokens_cleared: bool,
    pub metrics_cleared: bool,
}

#[tauri::command]
pub async fn factory_reset(
    core: State<'_, Arc<CoreContext>>,
) -> Result<FactoryResetResult, ErrorResponse> {
    tracing::info!("[factory_reset] Starting factory reset");

    let providers = core.provider_service.list_providers().await?;
    let providers_removed = providers.len();

    for provider in &providers {
        if let Err(e) = core.provider_service.remove_provider(provider.id).await {
            tracing::warn!(
                "[factory_reset] Failed to remove provider {}: {}",
                provider.id,
                e
            );
        }
    }

    if let Err(e) = core.pipeline_service.clear_all_caches_atomic().await {
        tracing::warn!("[factory_reset] Failed to clear caches: {}", e);
    }

    let metrics_cleared = if let Some(ref metrics_service) = core.metrics_service {
        match metrics_service.flush_metrics(None, true).await {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!("[factory_reset] Failed to clear metrics: {}", e);
                false
            }
        }
    } else {
        false
    };

    let tokens_cleared = match core.token_store.get_all_tokens().await {
        Ok(tokens) => {
            let mut success = true;
            for (provider_id, _) in tokens {
                if let Err(e) = core.token_store.delete_token(provider_id).await {
                    tracing::warn!(
                        "[factory_reset] Failed to delete token for provider {}: {}",
                        provider_id,
                        e
                    );
                    success = false;
                }
            }
            success
        }
        Err(e) => {
            tracing::warn!("[factory_reset] Failed to get tokens for cleanup: {}", e);
            false
        }
    };

    let mut legacy_cleaned = 0;
    for provider in &providers {
        use keyring::Entry;
        if let Ok(old_entry) = Entry::new("pipedash", &format!("provider_{}", provider.id)) {
            if old_entry.delete_credential().is_ok() {
                legacy_cleaned += 1;
                tracing::debug!(
                    "[factory_reset] Cleaned up legacy keyring entry for provider_{}",
                    provider.id
                );
            }
        }
    }
    if legacy_cleaned > 0 {
        tracing::info!(
            "[factory_reset] Cleaned up {} legacy keyring entries",
            legacy_cleaned
        );
    }

    tracing::info!(
        "[factory_reset] Complete - removed {} providers, caches cleared, tokens_cleared: {}, metrics_cleared: {}, legacy_entries_cleaned: {}",
        providers_removed, tokens_cleared, metrics_cleared, legacy_cleaned
    );

    Ok(FactoryResetResult {
        providers_removed,
        caches_cleared: true,
        tokens_cleared,
        metrics_cleared,
    })
}

#[tauri::command]
pub async fn restart_app(app: tauri::AppHandle) -> Result<(), ErrorResponse> {
    tracing::info!("[restart_app] Initiating application restart...");

    app.restart();

    #[allow(unreachable_code)]
    Ok(())
}
