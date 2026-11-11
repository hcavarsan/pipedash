use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::{
    Request,
    StatusCode,
};
use axum::Json;
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
use tokio::sync::Notify;
use tower::ServiceExt;

use super::*;
use crate::ws::event_bus::WebSocketEventBus;

#[derive(Clone)]
struct GatedStub {
    received: Arc<Notify>,
    release: Arc<Notify>,
}

async fn gated_runs(State(stub): State<GatedStub>) -> Json<Value> {
    stub.received.notify_one();
    stub.release.notified().await;
    stub.release.notify_one();

    Json(json!({ "runs": [{ "number": 1, "status": "success" }] }))
}

async fn post_status(app: &Router, uri: &str) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(Request::post(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}

#[tokio::test]
async fn refresh_all_does_not_hold_the_state_lock() {
    let _env = CONFIG_ENV.lock().await;
    let data_dir =
        std::env::temp_dir().join(format!("pipedash-refresh-lock-{}", std::process::id()));
    std::env::set_var("PIPEDASH_CONFIG_PATH", data_dir.join("config.toml"));
    let mut config = PipedashConfig::default();
    config.storage.backend = StorageBackendType::Sqlite;
    config.storage.data_dir = data_dir.display().to_string();
    config.storage.vault_password = Some("vault-password".into());
    let storage_manager = StorageManager::from_config(config, false).await.unwrap();

    let stub = GatedStub {
        received: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    };
    let server = Router::new()
        .route("/runs", get(gated_runs))
        .with_state(stub.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let server_url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, server).await.unwrap() });

    let provider = ProviderConfig {
        id: None,
        name: "stub".into(),
        provider_type: "stub".into(),
        token: String::new(),
        config: [("server_url".to_string(), server_url)].into(),
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
    let state = AppState::initialized(
        ws_event_bus,
        core,
        storage_manager,
        crate::test_support::registry(),
    );
    let app = Router::new()
        .nest("/refresh", refresh::router())
        .nest("/vault", vault::router())
        .with_state(state);

    let refresh = tokio::spawn({
        let app = app.clone();
        async move { post_status(&app, "/refresh/all").await }
    });
    tokio::time::timeout(Duration::from_secs(10), stub.received.notified())
        .await
        .expect("refresh_all did not reach the provider");

    let (lock_status, _) =
        tokio::time::timeout(Duration::from_secs(2), post_status(&app, "/vault/lock"))
            .await
            .expect("vault lock blocked while refresh_all was in flight");
    stub.release.notify_one();
    tokio::time::timeout(Duration::from_secs(30), refresh)
        .await
        .expect("refresh_all did not finish")
        .unwrap();
    let _ = std::fs::remove_dir_all(&data_dir);

    assert_eq!(lock_status, StatusCode::OK);
    let (after_lock, body) = post_status(&app, "/refresh/all").await;
    assert_eq!(after_lock, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "NOT_INITIALIZED");
    let status = app
        .clone()
        .oneshot(Request::get("/vault/status").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);
    let status = axum::body::to_bytes(status.into_body(), usize::MAX)
        .await
        .unwrap();
    let status: Value = serde_json::from_slice(&status).unwrap();
    assert_eq!(status["is_unlocked"], false);
    assert_eq!(status["backend"], "sqlite");
}
