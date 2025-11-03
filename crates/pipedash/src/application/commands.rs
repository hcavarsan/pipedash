use std::collections::HashMap;
use std::sync::Arc;

use serde::{
    Deserialize,
    Serialize,
};
use tauri::{
    Emitter,
    State,
};

use crate::application::services::{
    MetricsService,
    PipelineService,
    ProviderService,
};
use crate::application::RefreshManager;
use crate::domain::{
    AggregatedMetrics,
    AggregationPeriod,
    AggregationType,
    AvailablePipeline,
    GlobalMetricsConfig,
    MetricType,
    MetricsConfig,
    MetricsQuery,
    MetricsStats,
    Pipeline,
    PipelineRun,
    ProviderConfig,
    ProviderSummary,
    TriggerParams,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

impl From<crate::domain::DomainError> for ErrorResponse {
    fn from(err: crate::domain::DomainError) -> Self {
        ErrorResponse {
            error: err.to_string(),
        }
    }
}

pub struct AppState {
    pub provider_service: Arc<ProviderService>,
    pub pipeline_service: Arc<PipelineService>,
    pub metrics_service: Option<Arc<MetricsService>>,
    pub refresh_manager: Arc<RefreshManager>,
    pub app: tauri::AppHandle,
}

#[tauri::command]
pub async fn add_provider(
    state: State<'_, AppState>, config: ProviderConfig,
) -> Result<i64, ErrorResponse> {
    let id = state
        .provider_service
        .add_provider(config)
        .await
        .map_err(ErrorResponse::from)?;

    let _ = state.pipeline_service.fetch_pipelines(Some(id)).await;

    let _ = state.app.emit("providers-changed", ());

    Ok(id)
}

#[tauri::command]
pub async fn list_providers(
    state: State<'_, AppState>,
) -> Result<Vec<ProviderSummary>, ErrorResponse> {
    let result = state
        .provider_service
        .list_providers()
        .await
        .map_err(ErrorResponse::from)?;
    Ok(result)
}

#[tauri::command]
pub async fn get_provider(
    state: State<'_, AppState>, id: i64,
) -> Result<ProviderConfig, ErrorResponse> {
    let result = state
        .provider_service
        .get_provider_config(id)
        .await
        .map_err(ErrorResponse::from)?;
    Ok(result)
}

#[tauri::command]
pub async fn update_provider(
    state: State<'_, AppState>, id: i64, config: ProviderConfig,
) -> Result<(), ErrorResponse> {
    state
        .provider_service
        .update_provider(id, config)
        .await
        .map_err(ErrorResponse::from)?;

    let _ = state.app.emit("providers-changed", ());

    Ok(())
}

#[tauri::command]
pub async fn update_provider_refresh_interval(
    state: State<'_, AppState>, id: i64, refresh_interval: i64,
) -> Result<(), ErrorResponse> {
    if refresh_interval < 5 {
        return Err(ErrorResponse {
            error: "Refresh interval must be at least 5 seconds".to_string(),
        });
    }

    state
        .provider_service
        .update_provider_refresh_interval(id, refresh_interval)
        .await
        .map_err(ErrorResponse::from)?;

    let _ = state.app.emit("providers-changed", ());

    Ok(())
}

#[tauri::command]
pub async fn remove_provider(state: State<'_, AppState>, id: i64) -> Result<(), ErrorResponse> {
    state
        .provider_service
        .remove_provider(id)
        .await
        .map_err(ErrorResponse::from)?;

    let _ = state.app.emit("providers-changed", ());

    Ok(())
}

#[tauri::command]
pub async fn fetch_pipelines(
    state: State<'_, AppState>, provider_id: Option<i64>,
) -> Result<Vec<Pipeline>, ErrorResponse> {
    state
        .pipeline_service
        .fetch_pipelines(provider_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_cached_pipelines(
    state: State<'_, AppState>, provider_id: Option<i64>,
) -> Result<Vec<Pipeline>, ErrorResponse> {
    state
        .pipeline_service
        .get_cached_pipelines(provider_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn fetch_run_history(
    state: State<'_, AppState>, pipeline_id: String, page: Option<usize>, page_size: Option<usize>,
) -> Result<crate::domain::PaginatedRunHistory, ErrorResponse> {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).clamp(1, 100);

    eprintln!(
        "[COMMAND] fetch_run_history: pipeline={}, page={}, page_size={}",
        pipeline_id, page, page_size
    );

    state
        .pipeline_service
        .fetch_run_history_paginated(&pipeline_id, page, page_size, Some(state.app.clone()))
        .await
        .map_err(ErrorResponse::from)
}

#[tauri::command]
pub async fn trigger_pipeline(
    state: State<'_, AppState>, params: TriggerParams,
) -> Result<String, ErrorResponse> {
    let workflow_id = params.workflow_id.clone();
    let result = state
        .pipeline_service
        .trigger_pipeline(params)
        .await
        .map_err(ErrorResponse::from)?;

    state
        .pipeline_service
        .invalidate_run_cache(&workflow_id)
        .await;
    let _ = state.app.emit("run-triggered", &workflow_id);

    Ok(result)
}

#[tauri::command]
pub async fn refresh_all(state: State<'_, AppState>) -> Result<(), ErrorResponse> {
    // Clear all run history caches on full refresh
    state.pipeline_service.clear_all_run_history_caches().await;

    state
        .pipeline_service
        .refresh_all()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn clear_run_history_cache(
    state: State<'_, AppState>, pipeline_id: String,
) -> Result<(), ErrorResponse> {
    state
        .pipeline_service
        .clear_run_history_cache(&pipeline_id)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn set_refresh_mode(
    state: State<'_, AppState>, mode: String,
) -> Result<(), ErrorResponse> {
    let refresh_mode = match mode.as_str() {
        "active" => crate::application::refresh_manager::RefreshMode::Active,
        "idle" => crate::application::refresh_manager::RefreshMode::Idle,
        _ => {
            return Err(ErrorResponse {
                error: format!("Invalid refresh mode: {mode}"),
            })
        }
    };

    state.refresh_manager.set_mode(refresh_mode).await;
    Ok(())
}

#[tauri::command]
pub async fn get_refresh_mode(state: State<'_, AppState>) -> Result<String, ErrorResponse> {
    let mode = state.refresh_manager.get_mode().await;
    Ok(match mode {
        crate::application::refresh_manager::RefreshMode::Active => "active".to_string(),
        crate::application::refresh_manager::RefreshMode::Idle => "idle".to_string(),
    })
}

#[tauri::command]
pub async fn get_workflow_run_details(
    state: State<'_, AppState>, pipeline_id: String, run_number: i64,
) -> Result<PipelineRun, ErrorResponse> {
    let run = state
        .pipeline_service
        .fetch_run_details(&pipeline_id, run_number)
        .await
        .map_err(ErrorResponse::from)?;

    eprintln!(
        "[COMMAND] Returning run details with inputs: {:?}",
        run.inputs.is_some()
    );
    if let Some(ref inputs) = run.inputs {
        eprintln!(
            "[COMMAND] Inputs JSON: {}",
            serde_json::to_string(inputs).unwrap_or_default()
        );
    }

    Ok(run)
}

#[tauri::command]
pub async fn cancel_pipeline_run(
    state: State<'_, AppState>, pipeline_id: String, run_number: i64,
) -> Result<(), ErrorResponse> {
    eprintln!("[COMMAND] Cancelling run #{run_number} for pipeline {pipeline_id}");

    state
        .pipeline_service
        .cancel_run(&pipeline_id, run_number)
        .await
        .map_err(ErrorResponse::from)?;

    state
        .pipeline_service
        .invalidate_run_cache(&pipeline_id)
        .await;
    let _ = state.app.emit("run-cancelled", &pipeline_id);

    eprintln!("[COMMAND] Run cancellation requested successfully");
    Ok(())
}

#[tauri::command]
pub async fn get_available_plugins(
    state: State<'_, AppState>,
) -> Result<Vec<pipedash_plugin_api::PluginMetadata>, ErrorResponse> {
    Ok(state.provider_service.list_available_plugins())
}

#[tauri::command]
pub async fn get_provider_field_options(
    provider_type: String, field_key: String, config: HashMap<String, String>,
    state: State<'_, AppState>,
) -> Result<Vec<String>, ErrorResponse> {
    let plugin = state
        .provider_service
        .create_uninitialized_plugin(&provider_type)
        .map_err(ErrorResponse::from)?;

    let options = plugin
        .get_field_options(&field_key, &config)
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to fetch field options: {e}"),
        })?;

    Ok(options)
}

#[tauri::command]
pub async fn preview_provider_pipelines(
    provider_type: String, config: HashMap<String, String>,
    state: State<'_, AppState>,
) -> Result<Vec<AvailablePipeline>, ErrorResponse> {
    let mut plugin = state
        .provider_service
        .create_uninitialized_plugin(&provider_type)
        .map_err(ErrorResponse::from)?;

    plugin
        .initialize(0, config)
        .map_err(|e| ErrorResponse {
            error: format!("Failed to initialize plugin: {e}"),
        })?;

    plugin
        .validate_credentials()
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to validate credentials: {e}"),
        })?;

    let plugin_pipelines = plugin
        .fetch_available_pipelines()
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to fetch available pipelines: {e}"),
        })?;

    let pipelines = plugin_pipelines
        .into_iter()
        .map(|p| AvailablePipeline {
            id: p.id,
            name: p.name,
            description: p.description,
            organization: p.organization,
            repository: p.repository,
        })
        .collect();

    Ok(pipelines)
}

#[tauri::command]
pub async fn get_workflow_parameters(
    workflow_id: String, state: tauri::State<'_, AppState>,
) -> Result<Vec<pipedash_plugin_api::WorkflowParameter>, ErrorResponse> {
    let parts: Vec<&str> = workflow_id.split("__").collect();
    if parts.len() < 2 {
        return Err(ErrorResponse {
            error: format!(
                "Invalid workflow ID format '{}'. Expected format: 'provider__id__...'",
                workflow_id
            ),
        });
    }

    let provider_id: i64 = parts[1].parse().map_err(|_| ErrorResponse {
        error: format!(
            "Invalid provider ID '{}' in workflow ID. Expected numeric ID.",
            parts[1]
        ),
    })?;

    let result = state
        .provider_service
        .get_workflow_parameters(provider_id, &workflow_id)
        .await
        .map_err(|e| ErrorResponse {
            error: format!("Failed to fetch workflow parameters: {e}"),
        })?;

    Ok(result)
}

#[tauri::command]
pub async fn get_global_metrics_config(
    state: State<'_, AppState>,
) -> Result<GlobalMetricsConfig, ErrorResponse> {
    let metrics_service = state
        .metrics_service
        .as_ref()
        .ok_or_else(|| ErrorResponse {
            error: "Metrics service not available".to_string(),
        })?;

    metrics_service
        .get_global_config()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn update_global_metrics_config(
    state: State<'_, AppState>, app_handle: tauri::AppHandle, enabled: bool,
    default_retention_days: i64,
) -> Result<(), ErrorResponse> {
    let metrics_service = state
        .metrics_service
        .as_ref()
        .ok_or_else(|| ErrorResponse {
            error: "Metrics service not available".to_string(),
        })?;

    metrics_service
        .update_global_config(enabled, default_retention_days)
        .await
        .map_err(ErrorResponse::from)?;

    let _ = app_handle.emit("metrics-global-config-changed", ());

    Ok(())
}

#[tauri::command]
pub async fn get_pipeline_metrics_config(
    state: State<'_, AppState>, pipeline_id: String,
) -> Result<MetricsConfig, ErrorResponse> {
    let metrics_service = state
        .metrics_service
        .as_ref()
        .ok_or_else(|| ErrorResponse {
            error: "Metrics service not available".to_string(),
        })?;

    metrics_service
        .get_pipeline_config(&pipeline_id)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn update_pipeline_metrics_config(
    state: State<'_, AppState>, app_handle: tauri::AppHandle, pipeline_id: String, enabled: bool,
    retention_days: i64,
) -> Result<(), ErrorResponse> {
    let metrics_service = state
        .metrics_service
        .as_ref()
        .ok_or_else(|| ErrorResponse {
            error: "Metrics service not available".to_string(),
        })?;

    metrics_service
        .update_pipeline_config(&pipeline_id, enabled, retention_days)
        .await
        .map_err(ErrorResponse::from)?;

    let _ = app_handle.emit("metrics-config-changed", &pipeline_id);

    Ok(())
}

#[tauri::command]
pub async fn query_pipeline_metrics(
    state: State<'_, AppState>, pipeline_id: Option<String>, metric_type: Option<String>,
    start_date: Option<String>, end_date: Option<String>, limit: Option<usize>,
) -> Result<Vec<crate::domain::MetricEntry>, ErrorResponse> {
    let metrics_service = state
        .metrics_service
        .as_ref()
        .ok_or_else(|| ErrorResponse {
            error: "Metrics service not available".to_string(),
        })?;

    let parsed_metric_type = metric_type.and_then(|t| MetricType::from_str(&t));
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
    state: State<'_, AppState>, pipeline_id: Option<String>, metric_type: String,
    aggregation_period: String, aggregation_type: Option<String>, start_date: Option<String>,
    end_date: Option<String>, limit: Option<usize>,
) -> Result<AggregatedMetrics, ErrorResponse> {
    let metrics_service = state
        .metrics_service
        .as_ref()
        .ok_or_else(|| ErrorResponse {
            error: "Metrics service not available".to_string(),
        })?;

    let parsed_metric_type = MetricType::from_str(&metric_type).ok_or_else(|| ErrorResponse {
        error: format!("Invalid metric type: {}", metric_type),
    })?;

    let parsed_aggregation = match aggregation_period.as_str() {
        "hourly" => AggregationPeriod::Hourly,
        "daily" => AggregationPeriod::Daily,
        "weekly" => AggregationPeriod::Weekly,
        "monthly" => AggregationPeriod::Monthly,
        _ => {
            return Err(ErrorResponse {
                error: format!("Invalid aggregation period: {}", aggregation_period),
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
    state: State<'_, AppState>,
) -> Result<MetricsStats, ErrorResponse> {
    let metrics_service = state
        .metrics_service
        .as_ref()
        .ok_or_else(|| ErrorResponse {
            error: "Metrics service not available".to_string(),
        })?;

    metrics_service
        .get_storage_stats()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn flush_pipeline_metrics(
    state: State<'_, AppState>, app_handle: tauri::AppHandle, pipeline_id: Option<String>,
) -> Result<usize, ErrorResponse> {
    let metrics_service = state
        .metrics_service
        .as_ref()
        .ok_or_else(|| ErrorResponse {
            error: "Metrics service not available".to_string(),
        })?;

    let deleted = metrics_service
        .flush_metrics(pipeline_id.clone())
        .await
        .map_err(ErrorResponse::from)?;

    let _ = app_handle.emit("metrics-flushed", &pipeline_id);

    Ok(deleted)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheStats {
    pub pipelines_count: i64,
    pub run_history_count: i64,
    pub workflow_params_count: i64,
    pub metrics_count: i64,
}

#[tauri::command]
pub async fn get_cache_stats(state: State<'_, AppState>) -> Result<CacheStats, ErrorResponse> {
    let pipelines_count = state
        .pipeline_service
        .get_pipelines_cache_count()
        .await
        .unwrap_or(0);

    let run_history_count = state
        .pipeline_service
        .get_run_history_cache_count()
        .await
        .unwrap_or(0);

    let workflow_params_count = state
        .pipeline_service
        .get_workflow_params_cache_count()
        .await
        .unwrap_or(0);

    let metrics_count = if let Some(metrics_service) = &state.metrics_service {
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
pub async fn clear_pipelines_cache(state: State<'_, AppState>) -> Result<usize, ErrorResponse> {
    state
        .pipeline_service
        .clear_pipelines_cache()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn clear_all_run_history_caches(state: State<'_, AppState>) -> Result<(), ErrorResponse> {
    state.pipeline_service.clear_all_run_history_caches().await;
    Ok(())
}

#[tauri::command]
pub async fn clear_workflow_params_cache(state: State<'_, AppState>) -> Result<(), ErrorResponse> {
    state
        .pipeline_service
        .clear_workflow_params_cache()
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn clear_all_caches(state: State<'_, AppState>) -> Result<(), ErrorResponse> {
    state.pipeline_service.clear_all_run_history_caches().await;

    state
        .pipeline_service
        .clear_pipelines_cache()
        .await
        .map_err(|e| ErrorResponse {
            error: e.to_string(),
        })?;

    state
        .pipeline_service
        .clear_workflow_params_cache()
        .await
        .map_err(|e| ErrorResponse {
            error: e.to_string(),
        })?;

    Ok(())
}
