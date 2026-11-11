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
use crate::ws::event_bus::WebSocketEventBus;

const LATEST_BUILD: i64 = 150;

#[derive(Clone, Default)]
struct RunsStub {
    reject: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    triggered: Arc<AtomicBool>,
}

impl RunsStub {
    fn latest_build(&self) -> i64 {
        if self.triggered.load(Ordering::SeqCst) {
            LATEST_BUILD + 1
        } else {
            LATEST_BUILD
        }
    }

    fn build(&self, number: i64) -> Value {
        if number == LATEST_BUILD && !self.stopped.load(Ordering::SeqCst) {
            return json!({ "number": number, "status": "running", "timestamp": 0, "duration": 0 });
        }
        if number == LATEST_BUILD {
            return json!({ "number": number, "status": "cancelled", "timestamp": 0, "duration": 1000 });
        }

        json!({ "number": number, "status": "success", "timestamp": 0, "duration": 1000 })
    }
}

async fn list_runs(State(stub): State<RunsStub>, RawQuery(query): RawQuery) -> Json<Value> {
    let limit: usize = query
        .as_deref()
        .and_then(|query| query.strip_prefix("limit="))
        .and_then(|limit| limit.parse().ok())
        .unwrap();
    let runs: Vec<Value> = (1..=stub.latest_build())
        .rev()
        .take(limit)
        .map(|number| stub.build(number))
        .collect();

    Json(json!({ "runs": runs }))
}

async fn run_details(State(stub): State<RunsStub>, Path(number): Path<i64>) -> Json<Value> {
    Json(stub.build(number))
}

async fn stop_run(State(stub): State<RunsStub>) -> StatusCode {
    stub.stopped.store(true, Ordering::SeqCst);

    StatusCode::OK
}

async fn trigger_run(State(stub): State<RunsStub>) -> StatusCode {
    stub.triggered.store(true, Ordering::SeqCst);

    StatusCode::CREATED
}

async fn workflow_params() -> Json<Value> {
    Json(json!([]))
}

async fn reject_when_set(
    State(reject): State<Arc<AtomicBool>>, request: Request<Body>, next: Next,
) -> Response {
    if reject.load(Ordering::SeqCst) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    next.run(request).await
}

async fn start_stub_server(stub: RunsStub) -> String {
    let reject = stub.reject.clone();
    let stub = Router::new()
        .route("/runs", get(list_runs))
        .route("/runs/{number}", get(run_details))
        .route("/runs/{number}/cancel", post(stop_run))
        .route("/trigger", post(trigger_run))
        .route("/params", get(workflow_params))
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

async fn app_with_stub(
    data_dir: &std::path::Path, stub: RunsStub,
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
        name: "stub".into(),
        provider_type: "stub".into(),
        token: String::new(),
        config: [("server_url".to_string(), start_stub_server(stub).await)].into(),
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
        .store_token(provider_id, "stub-token")
        .await
        .unwrap();

    let ws_event_bus = Arc::new(WebSocketEventBus::new());
    let core = CoreContext::with_storage_manager(
        &storage_manager,
        ws_event_bus.clone(),
        crate::test_support::registry(),
    )
    .await
    .unwrap();
    core.refresh_manager.start().await;
    let pipeline_service = core.pipeline_service.clone();
    let state = AppState::initialized(
        ws_event_bus,
        core,
        storage_manager,
        crate::test_support::registry(),
    );

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

async fn run_history(app: &Router, pipeline_id: &str, page: usize) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::post(format!("/{pipeline_id}/runs/query"))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "page": page, "page_size": 20 }).to_string(),
                ))
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
    let (app, provider_id, pipeline_service) = app_with_stub(&data_dir, RunsStub::default()).await;
    let pipeline_id = format!("stub__{provider_id}__build");

    fetch_now(&pipeline_service, provider_id).await;
    let (_, page) = run_history(&app, &pipeline_id, 6).await;
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
    let stub = RunsStub::default();
    let reject = stub.reject.clone();
    let (app, provider_id, pipeline_service) = app_with_stub(&data_dir, stub).await;
    let pipeline_id = format!("stub__{provider_id}__build");

    fetch_now(&pipeline_service, provider_id).await;
    reject.store(true, Ordering::SeqCst);
    let (status, body) = run_history(&app, &pipeline_id, 1).await;
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
    let (app, provider_id, pipeline_service) = app_with_stub(&data_dir, RunsStub::default()).await;
    let pipeline_id = format!("stub__{provider_id}__build");

    fetch_now(&pipeline_service, provider_id).await;
    let (_, before) = run_history(&app, &pipeline_id, 1).await;
    let cancel = post_status(
        &app,
        &format!("/{pipeline_id}/runs/{LATEST_BUILD}/cancel"),
        Value::Null,
    )
    .await;
    let (_, after) = run_history(&app, &pipeline_id, 1).await;
    let _ = std::fs::remove_dir_all(&data_dir);

    assert_eq!(before["runs"][0]["status"], "running");
    assert_eq!(cancel, StatusCode::OK);
    assert_eq!(after["runs"][0]["run_number"], LATEST_BUILD);
    assert_eq!(after["runs"][0]["status"], "cancelled");
}

#[tokio::test]
async fn run_history_shows_a_triggered_run() {
    let data_dir = std::env::temp_dir().join(format!("pipedash-trigger-{}", std::process::id()));
    let (app, provider_id, pipeline_service) = app_with_stub(&data_dir, RunsStub::default()).await;
    let pipeline_id = format!("stub__{provider_id}__build");

    fetch_now(&pipeline_service, provider_id).await;
    let (_, before) = run_history(&app, &pipeline_id, 1).await;
    let trigger = post_status(
        &app,
        &format!("/{pipeline_id}/trigger"),
        json!({ "workflow_id": pipeline_id }),
    )
    .await;
    let (_, after) = run_history(&app, &pipeline_id, 1).await;
    let _ = std::fs::remove_dir_all(&data_dir);

    assert_eq!(before["runs"][0]["run_number"], LATEST_BUILD);
    assert_eq!(trigger, StatusCode::OK);
    assert_eq!(after["runs"][0]["run_number"], LATEST_BUILD + 1);
}

#[tokio::test]
async fn prepare_trigger_keeps_the_provider_error_status() {
    let data_dir =
        std::env::temp_dir().join(format!("pipedash-workflow-params-{}", std::process::id()));
    let stub = RunsStub::default();
    let reject = stub.reject.clone();
    let (app, provider_id, pipeline_service) = app_with_stub(&data_dir, stub).await;
    fetch_now(&pipeline_service, provider_id).await;
    let missing = provider_id + 999;

    let missing_status = post_status(
        &app,
        &format!("/stub__{missing}__build/trigger/prepare"),
        json!({}),
    )
    .await;
    reject.store(true, Ordering::SeqCst);
    let rejected_status = post_status(
        &app,
        &format!("/stub__{provider_id}__build/trigger/prepare"),
        json!({}),
    )
    .await;
    let _ = std::fs::remove_dir_all(&data_dir);

    assert_eq!(missing_status, StatusCode::NOT_FOUND);
    assert_eq!(rejected_status, StatusCode::FORBIDDEN);
}
