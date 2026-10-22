use axum::{
    extract::{
        Path,
        Query,
        State,
    },
    routing::{
        get,
        post,
    },
    Json,
    Router,
};
use pipedash_core::domain::{
    PaginatedRunHistory,
    Pipeline,
    PipelineRun,
    TriggerParams,
};
use pipedash_plugin_api::{
    ReviewDecision,
    WorkflowParameter,
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
pub struct ListPipelinesQuery {
    pub provider_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct LazyPipelinesQuery {
    pub provider_id: Option<i64>,
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_lazy_page_size")]
    pub page_size: usize,
}

fn default_lazy_page_size() -> usize {
    20
}

#[derive(Debug, Deserialize)]
pub struct RunHistoryQuery {
    #[serde(default = "default_page")]
    pub page: usize,
    #[serde(default = "default_page_size")]
    pub page_size: usize,
}

fn default_page() -> usize {
    1
}

fn default_page_size() -> usize {
    20
}

#[derive(Debug, Deserialize)]
pub struct TriggerPipelineRequest {
    pub workflow_id: String,
    #[serde(default)]
    pub inputs: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ReviewRunRequest {
    pub decision: ReviewDecision,
}

#[derive(Debug, Serialize)]
pub struct TriggerResponse {
    pub run_id: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_pipelines))
        .route("/cached", get(get_cached_pipelines))
        .route("/fresh", get(fetch_fresh_pipelines))
        .route("/lazy", get(list_pipelines_lazy))
        .route("/{id}/runs", get(get_run_history))
        .route("/{id}/runs/{run_number}", get(get_run_details))
        .route("/{id}/trigger", post(trigger_pipeline))
        .route("/{id}/runs/{run_number}/cancel", post(cancel_run))
        .route("/{id}/runs/{run_number}/review", post(review_run))
        .route("/{id}/workflow-params", get(get_workflow_parameters))
}

async fn list_pipelines(
    State(state): State<AppState>, Query(query): Query<ListPipelinesQuery>,
) -> ApiResult<Json<Vec<Pipeline>>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let pipelines = core
        .pipeline_service
        .get_cached_pipelines(query.provider_id)
        .await?;

    let pipeline_service = core.pipeline_service.clone();
    let provider_id = query.provider_id;
    tokio::spawn(async move {
        let _ = pipeline_service.fetch_pipelines(provider_id).await;
    });

    Ok(Json(pipelines))
}

async fn get_run_history(
    State(state): State<AppState>, Path(pipeline_id): Path<String>,
    Query(query): Query<RunHistoryQuery>,
) -> ApiResult<Json<PaginatedRunHistory>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let result = core
        .pipeline_service
        .fetch_run_history_paginated(&pipeline_id, query.page, query.page_size)
        .await?;

    Ok(Json(result))
}

async fn get_run_details(
    State(state): State<AppState>, Path((pipeline_id, run_number)): Path<(String, i64)>,
) -> ApiResult<Json<PipelineRun>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let run = core
        .pipeline_service
        .fetch_run_details(&pipeline_id, run_number)
        .await?;

    Ok(Json(run))
}

async fn trigger_pipeline(
    State(state): State<AppState>, Path(_pipeline_id): Path<String>,
    Json(req): Json<TriggerPipelineRequest>,
) -> ApiResult<Json<TriggerResponse>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let params = TriggerParams {
        workflow_id: req.workflow_id,
        inputs: req.inputs,
    };

    let run_id = core.pipeline_service.trigger_pipeline(params).await?;

    Ok(Json(TriggerResponse { run_id }))
}

async fn cancel_run(
    State(state): State<AppState>, Path((pipeline_id, run_number)): Path<(String, i64)>,
) -> ApiResult<()> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    core.pipeline_service
        .cancel_run(&pipeline_id, run_number)
        .await?;
    Ok(())
}

async fn review_run(
    State(state): State<AppState>, Path((pipeline_id, run_number)): Path<(String, i64)>,
    Json(request): Json<ReviewRunRequest>,
) -> ApiResult<()> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    core.pipeline_service
        .review_run(&pipeline_id, run_number, request.decision)
        .await?;
    Ok(())
}

async fn get_cached_pipelines(
    State(state): State<AppState>, Query(query): Query<ListPipelinesQuery>,
) -> ApiResult<Json<Vec<Pipeline>>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let pipelines = core
        .pipeline_service
        .get_cached_pipelines(query.provider_id)
        .await?;
    Ok(Json(pipelines))
}

async fn fetch_fresh_pipelines(
    State(state): State<AppState>, Query(query): Query<ListPipelinesQuery>,
) -> ApiResult<Json<Vec<Pipeline>>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let pipelines = core
        .pipeline_service
        .fetch_pipelines(query.provider_id)
        .await?;
    Ok(Json(pipelines))
}

async fn list_pipelines_lazy(
    State(state): State<AppState>, Query(query): Query<LazyPipelinesQuery>,
) -> ApiResult<Json<pipedash_plugin_api::PaginatedResponse<Pipeline>>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let result = core
        .pipeline_service
        .fetch_pipelines_lazy(query.provider_id, query.page, query.page_size)
        .await?;
    Ok(Json(result))
}

async fn get_workflow_parameters(
    State(state): State<AppState>, Path(workflow_id): Path<String>,
) -> ApiResult<Json<Vec<WorkflowParameter>>> {
    let parts: Vec<&str> = workflow_id.split("__").collect();
    if parts.len() < 2 {
        return Err(AppError::bad_request(format!(
            "Invalid workflow ID format '{}'. Expected format: 'provider__id__...'",
            workflow_id
        )));
    }

    let provider_id: i64 = parts[1].parse().map_err(|_| {
        AppError::bad_request(format!(
            "Invalid provider ID '{}' in workflow ID. Expected numeric ID.",
            parts[1]
        ))
    })?;

    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let params = core
        .provider_service
        .get_workflow_parameters(provider_id, &workflow_id)
        .await
        .map_err(|e| AppError::internal(format!("Failed to fetch workflow parameters: {e}")))?;

    Ok(Json(params))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::extract::RawQuery;
    use axum::http::{
        Request,
        StatusCode,
    };
    use pipedash_core::infrastructure::{
        PipedashConfig,
        StorageBackendType,
        StorageManager,
    };
    use pipedash_core::{
        CoreContext,
        ProviderConfig,
    };
    use serde_json::{
        json,
        Value,
    };
    use tower::ServiceExt;

    use super::*;
    use crate::ws::WebSocketEventBus;

    const LATEST_BUILD: i64 = 150;

    fn jenkins_build(number: i64) -> Value {
        json!({ "number": number, "result": "SUCCESS", "timestamp": 0, "duration": 1000 })
    }

    async fn jenkins_job(RawQuery(query): RawQuery) -> Json<Value> {
        let query = query.unwrap_or_default();
        if !query.contains("builds") {
            return Json(json!({ "name": "build", "lastBuild": { "number": LATEST_BUILD } }));
        }
        let limit: i64 = query
            .rsplit(',')
            .next()
            .and_then(|tail| tail.split(['}', '%']).next())
            .and_then(|limit| limit.parse().ok())
            .unwrap();
        let builds: Vec<Value> = (1..=LATEST_BUILD)
            .rev()
            .take(limit as usize)
            .map(jenkins_build)
            .collect();

        Json(json!({ "builds": builds }))
    }

    async fn jenkins_build_details(Path(number): Path<i64>) -> Json<Value> {
        Json(jenkins_build(number))
    }

    async fn start_jenkins_stub() -> String {
        let stub = Router::new()
            .route("/job/build/api/json", get(jenkins_job))
            .route("/job/build/{number}/api/json", get(jenkins_build_details));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, stub).await.unwrap() });

        format!("http://{addr}")
    }

    async fn app_with_jenkins(data_dir: &std::path::Path) -> (Router, i64) {
        std::env::set_var("PIPEDASH_CONFIG_PATH", data_dir.join("config.toml"));
        let mut config = PipedashConfig::default();
        config.storage.backend = StorageBackendType::Sqlite;
        config.storage.data_dir = data_dir.display().to_string();
        config.storage.vault_password = Some("vault-password".into());
        let storage_manager = StorageManager::from_config(config, false).await.unwrap();

        let provider = ProviderConfig {
            id: None,
            name: "jenkins".into(),
            provider_type: "jenkins".into(),
            token: String::new(),
            config: [
                ("server_url".to_string(), start_jenkins_stub().await),
                ("username".to_string(), "admin".to_string()),
                ("selected_items".to_string(), "build".to_string()),
            ]
            .into(),
            refresh_interval: 30,
            version: None,
        };
        let provider_id = storage_manager
            .config_backend()
            .create_provider(&provider)
            .await
            .unwrap();
        storage_manager
            .token_store()
            .await
            .store_token(provider_id, "jenkins-token")
            .await
            .unwrap();

        let ws_event_bus = Arc::new(WebSocketEventBus::new());
        let core = CoreContext::with_storage_manager(&storage_manager, ws_event_bus.clone())
            .await
            .unwrap();
        let state = AppState::initialized(ws_event_bus, core, storage_manager);

        (router().with_state(state), provider_id)
    }

    async fn get_json(app: &Router, uri: &str) -> (StatusCode, Value) {
        let response = app
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();

        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn run_details_load_a_run_older_than_the_latest_100() {
        let data_dir =
            std::env::temp_dir().join(format!("pipedash-run-details-{}", std::process::id()));
        let (app, provider_id) = app_with_jenkins(&data_dir).await;
        let pipeline_id = format!("jenkins__{provider_id}__build");

        let (status, _) = get_json(&app, &format!("/fresh?provider_id={provider_id}")).await;
        assert_eq!(status, StatusCode::OK);
        let (_, page) = get_json(&app, &format!("/{pipeline_id}/runs?page=6&page_size=20")).await;
        let (status, run) = get_json(&app, &format!("/{pipeline_id}/runs/30")).await;
        let _ = std::fs::remove_dir_all(&data_dir);

        assert_eq!(page["runs"][0]["run_number"], 50);
        assert_eq!(status, StatusCode::OK, "{run}");
        assert_eq!(run["run_number"], 30);
    }
}
