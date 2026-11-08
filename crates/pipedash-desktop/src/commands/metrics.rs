use pipedash_core::domain::{
    GlobalMetricsConfig,
    MetricsConfig,
    MetricsDashboard,
    MetricsDashboardQuery,
    MetricsStats,
};
use tauri::State;

use crate::error::ErrorResponse;

#[tauri::command]
pub async fn get_global_metrics_config(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<GlobalMetricsConfig, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    let metrics_service = core.metrics_service.as_ref().ok_or_else(|| ErrorResponse {
        error: "Metrics service not available".to_string(),
        details: None,
        code: None,
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
        code: None,
    })?;
    let metrics_service = core.metrics_service.as_ref().ok_or_else(|| ErrorResponse {
        error: "Metrics service not available".to_string(),
        details: None,
        code: None,
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
        code: None,
    })?;
    let metrics_service = core.metrics_service.as_ref().ok_or_else(|| ErrorResponse {
        error: "Metrics service not available".to_string(),
        details: None,
        code: None,
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
        code: None,
    })?;
    let metrics_service = core.metrics_service.as_ref().ok_or_else(|| ErrorResponse {
        error: "Metrics service not available".to_string(),
        details: None,
        code: None,
    })?;

    metrics_service
        .update_pipeline_config(&pipeline_id, enabled, retention_days)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_metrics_dashboard(
    maybe_core: State<'_, crate::MaybeCoreContext>, query: MetricsDashboardQuery,
) -> Result<MetricsDashboard, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    let metrics_service = core.metrics_service.as_ref().ok_or_else(|| ErrorResponse {
        error: "Metrics service not available".to_string(),
        details: None,
        code: None,
    })?;

    metrics_service.dashboard(query).await.map_err(Into::into)
}

#[tauri::command]
pub async fn get_metrics_storage_stats(
    maybe_core: State<'_, crate::MaybeCoreContext>,
) -> Result<MetricsStats, ErrorResponse> {
    let core = maybe_core.get().await.map_err(|e| ErrorResponse {
        error: e,
        details: None,
        code: None,
    })?;
    let metrics_service = core.metrics_service.as_ref().ok_or_else(|| ErrorResponse {
        error: "Metrics service not available".to_string(),
        details: None,
        code: None,
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
        code: None,
    })?;
    let metrics_service = core.metrics_service.as_ref().ok_or_else(|| ErrorResponse {
        error: "Metrics service not available".to_string(),
        details: None,
        code: None,
    })?;

    metrics_service
        .flush_metrics(pipeline_id.as_deref(), false)
        .await
        .map_err(Into::into)
}
