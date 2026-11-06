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

fn parse_date_filter(
    value: Option<&str>, field: &str,
) -> ApiResult<Option<chrono::DateTime<chrono::Utc>>> {
    value
        .map(|value| {
            chrono::DateTime::parse_from_rfc3339(value)
                .map(|date| date.with_timezone(&chrono::Utc))
                .map_err(|_| {
                    AppError::bad_request(format!(
                        "Invalid {field}: expected an RFC 3339 date with a timezone"
                    ))
                })
        })
        .transpose()
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
        .map(|value| {
            value
                .parse::<MetricType>()
                .map_err(|_| AppError::bad_request(format!("Invalid metric type: {value}")))
        })
        .transpose()?;
    let parsed_start_date = parse_date_filter(params.start_date.as_deref(), "start_date")?;
    let parsed_end_date = parse_date_filter(params.end_date.as_deref(), "end_date")?;

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

    let parsed_start_date = parse_date_filter(params.start_date.as_deref(), "start_date")?;
    let parsed_end_date = parse_date_filter(params.end_date.as_deref(), "end_date")?;

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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{
        header,
        Request,
        StatusCode,
    };
    use pipedash_core::infrastructure::database::MetricsRepository;
    use pipedash_core::infrastructure::token_store::MemoryTokenStore;
    use pipedash_core::infrastructure::{
        ConfigLoader,
        StorageManager,
    };
    use pipedash_core::CoreContext;
    use serde_json::json;
    use tower::ServiceExt;

    use super::*;
    use crate::ws::WebSocketEventBus;

    async fn query(
        app: &Router, uri: &str, body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        let response = app
            .clone()
            .oneshot(
                Request::post(uri)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn metrics_filters_reject_invalid_values_and_preserve_optional_bounds() {
        const CHILD: &str = "PIPEDASH_METRICS_FILTER_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let root = std::env::temp_dir()
                .join(format!("pipedash-metrics-filters-{}", std::process::id()));
            std::fs::create_dir_all(&root).unwrap();
            let config_path = root.join("config.toml");
            std::fs::write(&config_path, format!(
                "[general]\nmetrics_enabled = true\nstatus_pages_enabled = false\n[storage]\nbackend = \"sqlite\"\ndata_dir = {:?}\n",
                root.to_string_lossy()
            )).unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "routes::metrics::tests::metrics_filters_reject_invalid_values_and_preserve_optional_bounds", "--nocapture"])
                .env(CHILD, "1")
                .env("PIPEDASH_CONFIG_PATH", config_path)
                .output().unwrap();
            std::fs::remove_dir_all(&root).unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        let config = ConfigLoader::load_default().unwrap();
        let manager =
            StorageManager::with_token_store(config, Arc::new(MemoryTokenStore::new()), true)
                .await
                .unwrap();
        let bus = Arc::new(WebSocketEventBus::new());
        let core = CoreContext::with_storage_manager(&manager, bus.clone())
            .await
            .unwrap();
        let repository = MetricsRepository::new_from_pool(manager.database_pool());
        let timestamp = "2026-11-02T00:01:00Z"
            .parse::<chrono::DateTime<chrono::Utc>>()
            .unwrap();
        let entries = [
            (MetricType::RunDuration, 60.0, "duration"),
            (MetricType::SuccessRate, 100.0, "success"),
        ]
        .into_iter()
        .map(|(metric_type, value, hash)| MetricEntry {
            id: 0,
            pipeline_id: "example".to_string(),
            run_number: 1,
            timestamp,
            metric_type,
            value,
            metadata: None,
            created_at: timestamp,
            run_hash: Some(hash.to_string()),
        })
        .collect();
        repository.insert_metrics_batch(entries).await.unwrap();
        let state = AppState::initialized(bus, core.clone(), manager);
        let app = router().with_state(state);

        for (uri, base) in [
            ("/pipelines/example/query", json!({})),
            (
                "/aggregated",
                json!({"pipeline_id": "example", "metric_type": "run_duration", "aggregation_period": "daily"}),
            ),
        ] {
            for (field, value) in [
                ("metric_type", "run_duraton"),
                ("start_date", "2026-11-02T00:02:00"),
                ("end_date", "2026-11-02T00:00:00"),
                ("start_date", ""),
            ] {
                let mut body = base.clone();
                body[field] = json!(value);
                let (status, error) = query(&app, uri, body).await;
                assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {field}: {error}");
                assert_eq!(error["code"], "BAD_REQUEST");
            }

            for body in [base.clone(), {
                let mut body = base.clone();
                body["start_date"] = json!("2026-11-01T21:01:00-03:00");
                body["end_date"] = json!("2026-11-02T00:01:00Z");
                body
            }] {
                let (status, result) = query(&app, uri, body).await;
                assert_eq!(status, StatusCode::OK);
                if uri == "/aggregated" {
                    assert_eq!(result["metrics"][0]["value"], 60.0);
                    assert_eq!(result["metrics"][0]["count"], 1);
                } else {
                    let types: std::collections::BTreeSet<_> = result
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|row| row["metric_type"].as_str().unwrap())
                        .collect();
                    assert_eq!(types, ["run_duration", "success_rate"].into());
                }
            }
            for (field, value) in [
                ("start_date", "2026-11-02T00:02:00Z"),
                ("end_date", "2026-11-02T00:00:00Z"),
            ] {
                let mut body = base.clone();
                body[field] = json!(value);
                let (status, result) = query(&app, uri, body).await;
                assert_eq!(status, StatusCode::OK);
                if uri == "/aggregated" {
                    assert_eq!(result["metrics"], json!([]));
                } else {
                    assert_eq!(result, json!([]));
                }
            }
        }
        let (status, result) = query(
            &app,
            "/pipelines/example/query",
            json!({"metric_type": "run_duration"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            result
                .as_array()
                .unwrap()
                .iter()
                .map(|row| (&row["metric_type"], &row["value"]))
                .collect::<Vec<_>>(),
            vec![(&json!("run_duration"), &json!(60.0))]
        );
        core.shutdown().await;
    }
}
