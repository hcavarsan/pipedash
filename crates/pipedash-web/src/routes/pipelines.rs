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
        .route("/{id}/runs", get(get_run_history))
        .route("/{id}/runs/{run_number}", get(get_run_details))
        .route("/{id}/trigger", post(trigger_pipeline))
        .route("/{id}/runs/{run_number}/cancel", post(cancel_run))
        .route("/{id}/runs/{run_number}/review", post(review_run))
        .route(
            "/{id}/runs/{run_number}/rerun-failed-jobs",
            post(rerun_failed_jobs),
        )
        .route("/{id}/workflow-params", get(get_workflow_parameters))
}

async fn list_pipelines(
    State(state): State<AppState>, Query(query): Query<ListPipelinesQuery>,
) -> ApiResult<Json<Vec<Pipeline>>> {
    let pipeline_service = state.pipeline_service().await?;
    let pipelines = pipeline_service
        .get_cached_pipelines(query.provider_id)
        .await?;
    Ok(Json(pipelines))
}

async fn get_run_history(
    State(state): State<AppState>, Path(pipeline_id): Path<String>,
    Query(query): Query<RunHistoryQuery>,
) -> ApiResult<Json<PaginatedRunHistory>> {
    let pipeline_service = state.pipeline_service().await?;
    let result = pipeline_service
        .fetch_run_history_paginated(&pipeline_id, query.page, query.page_size)
        .await?;

    Ok(Json(result))
}

async fn get_run_details(
    State(state): State<AppState>, Path((pipeline_id, run_number)): Path<(String, i64)>,
) -> ApiResult<Json<PipelineRun>> {
    let pipeline_service = state.pipeline_service().await?;
    let run = pipeline_service
        .fetch_run_details(&pipeline_id, run_number)
        .await?;

    Ok(Json(run))
}

async fn trigger_pipeline(
    State(state): State<AppState>, Path(_pipeline_id): Path<String>,
    Json(req): Json<TriggerPipelineRequest>,
) -> ApiResult<Json<TriggerResponse>> {
    let pipeline_service = state.pipeline_service().await?;
    let workflow_id = req.workflow_id.clone();
    let params = TriggerParams {
        workflow_id: req.workflow_id,
        inputs: req.inputs,
    };

    let run_id = pipeline_service.trigger_pipeline(params).await?;
    pipeline_service.invalidate_run_cache(&workflow_id).await;

    Ok(Json(TriggerResponse { run_id }))
}

async fn cancel_run(
    State(state): State<AppState>, Path((pipeline_id, run_number)): Path<(String, i64)>,
) -> ApiResult<()> {
    let pipeline_service = state.pipeline_service().await?;
    pipeline_service
        .cancel_run(&pipeline_id, run_number)
        .await?;
    pipeline_service.invalidate_run_cache(&pipeline_id).await;
    Ok(())
}

async fn review_run(
    State(state): State<AppState>, Path((pipeline_id, run_number)): Path<(String, i64)>,
    Json(request): Json<ReviewRunRequest>,
) -> ApiResult<()> {
    let pipeline_service = state.pipeline_service().await?;
    pipeline_service
        .review_run(&pipeline_id, run_number, request.decision)
        .await?;
    Ok(())
}

async fn rerun_failed_jobs(
    State(state): State<AppState>, Path((pipeline_id, run_number)): Path<(String, i64)>,
) -> ApiResult<()> {
    let pipeline_service = state.pipeline_service().await?;
    pipeline_service
        .rerun_failed_jobs(&pipeline_id, run_number)
        .await?;
    Ok(())
}

async fn get_cached_pipelines(
    State(state): State<AppState>, Query(query): Query<ListPipelinesQuery>,
) -> ApiResult<Json<Vec<Pipeline>>> {
    let pipeline_service = state.pipeline_service().await?;
    let pipelines = pipeline_service
        .get_cached_pipelines(query.provider_id)
        .await?;
    Ok(Json(pipelines))
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

    let provider_service = state.provider_service().await?;
    let params = provider_service
        .get_workflow_parameters(provider_id, &workflow_id)
        .await
        .map_err(|e| AppError::internal(format!("Failed to fetch workflow parameters: {e}")))?;

    Ok(Json(params))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{
        AtomicBool,
        Ordering,
    };
    use std::sync::Arc;

    use axum::body::Body;
    use axum::extract::RawQuery;
    use axum::http::{
        Request,
        StatusCode,
    };
    use axum::middleware::Next;
    use axum::response::{
        IntoResponse,
        Response,
    };
    use pipedash_core::application::PipelineService;
    use pipedash_core::domain::RefreshState;
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

    #[derive(Clone, Default)]
    struct JenkinsStub {
        reject: Arc<AtomicBool>,
        stopped: Arc<AtomicBool>,
        triggered: Arc<AtomicBool>,
    }

    impl JenkinsStub {
        fn latest_build(&self) -> i64 {
            if self.triggered.load(Ordering::SeqCst) {
                LATEST_BUILD + 1
            } else {
                LATEST_BUILD
            }
        }

        fn build(&self, number: i64) -> Value {
            if number == LATEST_BUILD && !self.stopped.load(Ordering::SeqCst) {
                return json!({ "number": number, "result": null, "building": true, "timestamp": 0, "duration": 0 });
            }
            if number == LATEST_BUILD {
                return json!({ "number": number, "result": "ABORTED", "building": false, "timestamp": 0, "duration": 1000 });
            }

            json!({ "number": number, "result": "SUCCESS", "timestamp": 0, "duration": 1000 })
        }
    }

    async fn jenkins_job(
        State(stub): State<JenkinsStub>, RawQuery(query): RawQuery,
    ) -> Json<Value> {
        let query = query.unwrap_or_default();
        if !query.contains("builds") {
            return Json(
                json!({ "name": "build", "lastBuild": { "number": stub.latest_build() } }),
            );
        }
        let limit: i64 = query
            .rsplit(',')
            .next()
            .and_then(|tail| tail.split(['}', '%']).next())
            .and_then(|limit| limit.parse().ok())
            .unwrap();
        let builds: Vec<Value> = (1..=stub.latest_build())
            .rev()
            .take(limit as usize)
            .map(|number| stub.build(number))
            .collect();

        Json(json!({ "builds": builds }))
    }

    async fn jenkins_build_details(
        State(stub): State<JenkinsStub>, Path(number): Path<i64>,
    ) -> Json<Value> {
        Json(stub.build(number))
    }

    async fn jenkins_stop(State(stub): State<JenkinsStub>) -> StatusCode {
        stub.stopped.store(true, Ordering::SeqCst);

        StatusCode::FOUND
    }

    async fn jenkins_trigger(State(stub): State<JenkinsStub>) -> StatusCode {
        stub.triggered.store(true, Ordering::SeqCst);

        StatusCode::CREATED
    }

    async fn reject_when_set(
        State(reject): State<Arc<AtomicBool>>, request: Request<Body>, next: Next,
    ) -> Response {
        if reject.load(Ordering::SeqCst) {
            return StatusCode::UNAUTHORIZED.into_response();
        }
        next.run(request).await
    }

    async fn start_jenkins_stub(stub: JenkinsStub) -> String {
        let reject = stub.reject.clone();
        let stub = Router::new()
            .route("/job/build/api/json", get(jenkins_job))
            .route("/job/build/{number}/api/json", get(jenkins_build_details))
            .route("/job/build/{number}/stop", post(jenkins_stop))
            .route("/job/build/build", post(jenkins_trigger))
            .with_state(stub)
            .layer(axum::middleware::from_fn_with_state(
                reject,
                reject_when_set,
            ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, stub).await.unwrap() });

        format!("http://{addr}")
    }

    async fn app_with_jenkins(
        data_dir: &std::path::Path, stub: JenkinsStub,
    ) -> (Router, i64, Arc<PipelineService>) {
        let _env = crate::routes::CONFIG_ENV.lock().await;
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
                ("server_url".to_string(), start_jenkins_stub(stub).await),
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
        core.refresh_manager.start().await;
        let pipeline_service = core.pipeline_service.clone();
        let state = AppState::initialized(ws_event_bus, core, storage_manager);

        (router().with_state(state), provider_id, pipeline_service)
    }

    async fn fetch_now(pipeline_service: &PipelineService, provider_id: i64) {
        let results = pipeline_service
            .fetch_now(&[provider_id], PipelineService::REFRESH_DEADLINE)
            .await;
        assert_eq!(results[0].state, RefreshState::Ok, "{results:?}");
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

    async fn post_status(app: &Router, uri: &str, body: Value) -> StatusCode {
        let response = app
            .clone()
            .oneshot(
                Request::post(uri)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        response.status()
    }

    #[tokio::test]
    async fn run_details_load_a_run_older_than_the_latest_100() {
        let data_dir =
            std::env::temp_dir().join(format!("pipedash-run-details-{}", std::process::id()));
        let (app, provider_id, pipeline_service) =
            app_with_jenkins(&data_dir, JenkinsStub::default()).await;
        let pipeline_id = format!("jenkins__{provider_id}__build");

        fetch_now(&pipeline_service, provider_id).await;
        let (_, page) = get_json(&app, &format!("/{pipeline_id}/runs?page=6&page_size=20")).await;
        let (status, run) = get_json(&app, &format!("/{pipeline_id}/runs/30")).await;
        let _ = std::fs::remove_dir_all(&data_dir);

        assert_eq!(page["runs"][0]["run_number"], 50);
        assert_eq!(status, StatusCode::OK, "{run}");
        assert_eq!(run["run_number"], 30);
    }

    #[tokio::test]
    async fn rejected_provider_token_is_not_a_failed_api_login() {
        let data_dir =
            std::env::temp_dir().join(format!("pipedash-rejected-token-{}", std::process::id()));
        let stub = JenkinsStub::default();
        let reject = stub.reject.clone();
        let (app, provider_id, pipeline_service) = app_with_jenkins(&data_dir, stub).await;
        let pipeline_id = format!("jenkins__{provider_id}__build");

        fetch_now(&pipeline_service, provider_id).await;
        reject.store(true, Ordering::SeqCst);
        let (status, body) = get_json(&app, &format!("/{pipeline_id}/runs")).await;
        let _ = std::fs::remove_dir_all(&data_dir);

        assert_ne!(status, StatusCode::UNAUTHORIZED, "{body}");
        assert!(status.is_client_error(), "{status} {body}");
        assert!(
            body["message"]
                .as_str()
                .unwrap()
                .contains("Authentication failed"),
            "{body}"
        );
    }

    #[tokio::test]
    async fn run_history_shows_a_cancelled_run_as_cancelled() {
        let data_dir = std::env::temp_dir().join(format!("pipedash-cancel-{}", std::process::id()));
        let (app, provider_id, pipeline_service) =
            app_with_jenkins(&data_dir, JenkinsStub::default()).await;
        let pipeline_id = format!("jenkins__{provider_id}__build");
        let history = format!("/{pipeline_id}/runs?page=1&page_size=20");

        fetch_now(&pipeline_service, provider_id).await;
        let (_, before) = get_json(&app, &history).await;
        let cancel = post_status(
            &app,
            &format!("/{pipeline_id}/runs/{LATEST_BUILD}/cancel"),
            Value::Null,
        )
        .await;
        let (_, after) = get_json(&app, &history).await;
        let _ = std::fs::remove_dir_all(&data_dir);

        assert_eq!(before["runs"][0]["status"], "running");
        assert_eq!(cancel, StatusCode::OK);
        assert_eq!(after["runs"][0]["run_number"], LATEST_BUILD);
        assert_eq!(after["runs"][0]["status"], "cancelled");
    }

    #[tokio::test]
    async fn run_history_shows_a_triggered_run() {
        let data_dir =
            std::env::temp_dir().join(format!("pipedash-trigger-{}", std::process::id()));
        let (app, provider_id, pipeline_service) =
            app_with_jenkins(&data_dir, JenkinsStub::default()).await;
        let pipeline_id = format!("jenkins__{provider_id}__build");
        let history = format!("/{pipeline_id}/runs?page=1&page_size=20");

        fetch_now(&pipeline_service, provider_id).await;
        let (_, before) = get_json(&app, &history).await;
        let trigger = post_status(
            &app,
            &format!("/{pipeline_id}/trigger"),
            json!({ "workflow_id": pipeline_id }),
        )
        .await;
        let (_, after) = get_json(&app, &history).await;
        let _ = std::fs::remove_dir_all(&data_dir);

        assert_eq!(before["runs"][0]["run_number"], LATEST_BUILD);
        assert_eq!(trigger, StatusCode::OK);
        assert_eq!(after["runs"][0]["run_number"], LATEST_BUILD + 1);
    }
}
