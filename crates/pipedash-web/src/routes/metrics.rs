use axum::{
    extract::{
        Path,
        State,
    },
    routing::{
        get,
        post,
        put,
    },
    Json,
    Router,
};
use pipedash_core::domain::{
    AggregatedMetrics,
    AggregationPeriod,
    AggregationType,
    GlobalMetricsConfig,
    MetricEntry,
    MetricType,
    MetricsConfig,
    MetricsQuery,
    MetricsStats,
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
pub struct UpdateGlobalMetricsConfigRequest {
    pub enabled: bool,
    pub default_retention_days: i64,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePipelineMetricsConfigRequest {
    pub enabled: bool,
    pub retention_days: i64,
}

#[derive(Debug, Deserialize)]
pub struct MetricsQueryParams {
    pub metric_type: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct AggregatedMetricsQueryParams {
    pub pipeline_id: Option<String>,
    pub metric_type: String,
    pub aggregation_period: String,
    pub aggregation_type: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct FlushMetricsRequest {
    pub pipeline_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FlushMetricsResponse {
    pub flushed: usize,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/config", get(get_global_metrics_config))
        .route("/config", put(update_global_metrics_config))
        .route("/pipelines/{id}/config", get(get_pipeline_metrics_config))
        .route(
            "/pipelines/{id}/config",
            put(update_pipeline_metrics_config),
        )
        .route("/pipelines/{id}/query", post(query_pipeline_metrics))
        .route("/aggregated", post(query_aggregated_metrics))
        .route("/storage/stats", get(get_metrics_storage_stats))
        .route("/flush", post(flush_pipeline_metrics))
}

async fn get_global_metrics_config(
    State(state): State<AppState>,
) -> ApiResult<Json<GlobalMetricsConfig>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let metrics_service = core
        .metrics_service
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Metrics service not available"))?;
    let config = metrics_service.get_global_config().await?;
    Ok(Json(config))
}

async fn update_global_metrics_config(
    State(state): State<AppState>, Json(req): Json<UpdateGlobalMetricsConfigRequest>,
) -> ApiResult<()> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let metrics_service = core
        .metrics_service
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Metrics service not available"))?;
    metrics_service
        .update_global_config(req.enabled, req.default_retention_days)
        .await?;
    Ok(())
}

async fn get_pipeline_metrics_config(
    State(state): State<AppState>, Path(pipeline_id): Path<String>,
) -> ApiResult<Json<MetricsConfig>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let metrics_service = core
        .metrics_service
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Metrics service not available"))?;
    let config = metrics_service
        .get_effective_pipeline_config(&pipeline_id)
        .await?;
    Ok(Json(config))
}

async fn update_pipeline_metrics_config(
    State(state): State<AppState>, Path(pipeline_id): Path<String>,
    Json(req): Json<UpdatePipelineMetricsConfigRequest>,
) -> ApiResult<()> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let metrics_service = core
        .metrics_service
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Metrics service not available"))?;
    metrics_service
        .update_pipeline_config(&pipeline_id, req.enabled, req.retention_days)
        .await?;
    Ok(())
}

async fn query_pipeline_metrics(
    State(state): State<AppState>, Path(pipeline_id): Path<String>,
    Json(params): Json<MetricsQueryParams>,
) -> ApiResult<Json<Vec<MetricEntry>>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let metrics_service = core
        .metrics_service
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Metrics service not available"))?;

    let parsed_metric_type = params
        .metric_type
        .and_then(|t| t.parse::<MetricType>().ok());
    let parsed_start_date = params
        .start_date
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));
    let parsed_end_date = params
        .end_date
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let query = MetricsQuery {
        pipeline_id: Some(pipeline_id),
        metric_type: parsed_metric_type,
        start_date: parsed_start_date,
        end_date: parsed_end_date,
        aggregation_period: None,
        aggregation_type: None,
        limit: params.limit,
    };

    let metrics = metrics_service.query_metrics(query).await?;
    Ok(Json(metrics))
}

async fn query_aggregated_metrics(
    State(state): State<AppState>, Json(params): Json<AggregatedMetricsQueryParams>,
) -> ApiResult<Json<AggregatedMetrics>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let metrics_service = core
        .metrics_service
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Metrics service not available"))?;

    let parsed_metric_type = params.metric_type.parse::<MetricType>().map_err(|_| {
        AppError::bad_request(format!("Invalid metric type: {}", params.metric_type))
    })?;

    let parsed_aggregation = match params.aggregation_period.as_str() {
        "hourly" => AggregationPeriod::Hourly,
        "daily" => AggregationPeriod::Daily,
        "weekly" => AggregationPeriod::Weekly,
        "monthly" => AggregationPeriod::Monthly,
        _ => {
            return Err(AppError::bad_request(format!(
                "Invalid aggregation period: {}",
                params.aggregation_period
            )))
        }
    };

    let parsed_aggregation_type = params
        .aggregation_type
        .as_ref()
        .map(|s| match s.as_str() {
            "avg" => Ok(AggregationType::Avg),
            "sum" => Ok(AggregationType::Sum),
            "min" => Ok(AggregationType::Min),
            "max" => Ok(AggregationType::Max),
            "p95" => Ok(AggregationType::P95),
            "p99" => Ok(AggregationType::P99),
            _ => Err(AppError::bad_request(format!(
                "Invalid aggregation type: {}",
                s
            ))),
        })
        .transpose()?;

    let parsed_start_date = params
        .start_date
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));
    let parsed_end_date = params
        .end_date
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let query = MetricsQuery {
        pipeline_id: params.pipeline_id,
        metric_type: Some(parsed_metric_type),
        start_date: parsed_start_date,
        end_date: parsed_end_date,
        aggregation_period: Some(parsed_aggregation),
        aggregation_type: parsed_aggregation_type,
        limit: params.limit,
    };

    let aggregated = metrics_service.query_aggregated_metrics(query).await?;
    Ok(Json(aggregated))
}

async fn get_metrics_storage_stats(State(state): State<AppState>) -> ApiResult<Json<MetricsStats>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let metrics_service = core
        .metrics_service
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Metrics service not available"))?;
    let stats = metrics_service.get_storage_stats().await?;
    Ok(Json(stats))
}

async fn flush_pipeline_metrics(
    State(state): State<AppState>, Json(req): Json<FlushMetricsRequest>,
) -> ApiResult<Json<FlushMetricsResponse>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let metrics_service = core
        .metrics_service
        .as_ref()
        .ok_or_else(|| AppError::bad_request("Metrics service not available"))?;
    let flushed = metrics_service
        .flush_metrics(req.pipeline_id.as_deref(), false)
        .await?;
    Ok(Json(FlushMetricsResponse { flushed }))
}
