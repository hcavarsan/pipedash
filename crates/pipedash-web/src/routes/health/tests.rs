use std::sync::Arc;

use axum::body::Body;
use axum::http::{
    Request,
    StatusCode,
};
use axum::routing::get;
use axum::Router;
use pipedash_core::infrastructure::{
    PipedashConfig,
    StorageBackendType,
    StorageManager,
};
use pipedash_core::CoreContext;
use tower::ServiceExt;

use super::*;
use crate::ws::event_bus::WebSocketEventBus;

async fn get_health(app: &Router) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(Request::get("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn health_reports_a_failed_database_query_as_unavailable() {
    let _env = crate::routes::CONFIG_ENV.lock().await;
    let data_dir = std::env::temp_dir().join(format!("pipedash-health-{}", std::process::id()));
    std::env::set_var("PIPEDASH_CONFIG_PATH", data_dir.join("config.toml"));
    let mut config = PipedashConfig::default();
    config.storage.backend = StorageBackendType::Sqlite;
    config.storage.data_dir = data_dir.display().to_string();
    config.storage.vault_password = Some("vault-password".into());
    let storage_manager = StorageManager::from_config(config, false).await.unwrap();
    let ws_event_bus = Arc::new(WebSocketEventBus::new());
    let core = CoreContext::with_storage_manager(
        &storage_manager,
        ws_event_bus.clone(),
        crate::test_support::registry(),
    )
    .await
    .unwrap();
    let pool = core
        .provider_service
        .repository()
        .cache_pool()
        .as_sqlite()
        .unwrap()
        .clone();
    let state = AppState::initialized(
        ws_event_bus,
        core,
        storage_manager,
        crate::test_support::registry(),
    );
    state.inner.write().await.token_store_ready = true;
    let app = Router::new()
        .route("/health", get(health_check))
        .with_state(state);

    let (status, body) = get_health(&app).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["database"]["connected"], true);

    pool.close().await;

    let (failed_status, failed_body) = get_health(&app).await;
    let _ = std::fs::remove_dir_all(&data_dir);
    assert_eq!(failed_status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(failed_body["status"], "database_error");
    assert_eq!(failed_body["database"]["connected"], false);
}
