use std::sync::Arc;

use axum::body::Body;
use axum::http::{
    Request,
    StatusCode,
};
use tower::ServiceExt;

use super::*;
use crate::ws::event_bus::WebSocketEventBus;

fn draft() -> SetupDraft {
    SetupDraft {
        backend: "sqlite".into(),
        data_dir: String::new(),
        postgres_url: String::new(),
        vault_password: "attacker-password".into(),
        vault_password_confirm: "attacker-password".into(),
        transfer_data: false,
    }
}

#[tokio::test]
async fn apply_draft_rejects_configured_instance() {
    let state = AppState::config_error(
        Arc::new(WebSocketEventBus::new()),
        "invalid".into(),
        crate::test_support::registry(),
    );

    let err = apply_draft(State(state), Json(draft())).await.unwrap_err();

    assert_eq!(err.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn apply_draft_rechecks_after_waiting_for_running_setup() {
    let config_dir =
        std::env::temp_dir().join(format!("pipedash-setup-race-{}", std::process::id()));
    let _env = crate::routes::CONFIG_ENV.lock().await;
    std::env::set_var("PIPEDASH_CONFIG_PATH", config_dir.join("config.toml"));
    let state = AppState::setup_mode(
        Arc::new(WebSocketEventBus::new()),
        crate::test_support::registry(),
    );
    let running_setup = state.setup_lock.clone().lock_owned().await;
    let waiting_setup = tokio::spawn(apply_draft(State(state.clone()), Json(draft())));
    tokio::task::yield_now().await;

    state.inner.write().await.setup_required = false;
    drop(running_setup);
    let result = waiting_setup.await.unwrap();
    let _ = std::fs::remove_dir_all(&config_dir);

    assert_eq!(result.unwrap_err().status, StatusCode::CONFLICT);
}

async fn get_json(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let response = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    (status, serde_json::from_slice(&body).unwrap_or_default())
}

#[tokio::test]
async fn status_check_after_fixing_the_config_starts_the_services() {
    let _env = crate::routes::CONFIG_ENV.lock().await;
    let data_dir =
        std::env::temp_dir().join(format!("pipedash-config-fixed-{}", std::process::id()));
    let config_path = data_dir.join("config.toml");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(&config_path, "[storage\nbackend = \"sqlite\"\n").unwrap();
    std::env::set_var("PIPEDASH_CONFIG_PATH", &config_path);
    let app = Router::new()
        .nest("/setup", router())
        .nest("/providers", crate::routes::providers::router())
        .with_state(AppState::config_error(
            Arc::new(WebSocketEventBus::new()),
            "invalid".into(),
            crate::test_support::registry(),
        ));

    let (broken_status, broken_body) = get_json(&app, "/setup/status").await;
    let mut config = PipedashConfig::default();
    config.storage.backend = pipedash_core::infrastructure::StorageBackendType::Sqlite;
    config.storage.data_dir = data_dir.display().to_string();
    config.storage.vault_password = Some("vault-password".into());
    std::fs::write(&config_path, ConfigLoader::to_toml(&config).unwrap()).unwrap();
    let (fixed_status, fixed_body) = get_json(&app, "/setup/status").await;
    let (providers_status, providers_body) = get_json(&app, "/providers").await;
    let _ = std::fs::remove_dir_all(&data_dir);

    assert_eq!(broken_status, StatusCode::OK);
    assert_eq!(broken_body["config_valid"], false, "{broken_body}");
    assert_eq!(fixed_status, StatusCode::OK);
    assert_eq!(fixed_body["config_valid"], true, "{fixed_body}");
    assert_eq!(providers_status, StatusCode::OK, "{providers_body}");
}

async fn post_json(
    app: &Router, uri: &str, body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let request = Request::post(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

#[tokio::test]
async fn applying_a_draft_completes_setup_and_starts_the_core() {
    let _env = crate::routes::CONFIG_ENV.lock().await;
    let data_dir =
        std::env::temp_dir().join(format!("pipedash-setup-draft-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    let saved = [
        "PIPEDASH_CONFIG_PATH",
        "PIPEDASH_DATA_DIR",
        "PIPEDASH_VAULT_PASSWORD",
    ]
    .map(|name| (name, std::env::var_os(name)));
    std::env::set_var("PIPEDASH_CONFIG_PATH", data_dir.join("config.toml"));
    std::env::set_var("PIPEDASH_DATA_DIR", &data_dir);
    std::env::set_var("PIPEDASH_VAULT_PASSWORD", "x");
    let state = AppState::setup_mode(
        Arc::new(WebSocketEventBus::new()),
        crate::test_support::registry(),
    );
    let app = Router::new()
        .nest("/setup", router())
        .nest("/plugins", crate::routes::plugins::router())
        .with_state(state.clone());
    let body = serde_json::json!({
        "backend": "sqlite",
        "data_dir": data_dir.display().to_string(),
        "postgres_url": "",
        "vault_password": "x",
        "vault_password_confirm": "x",
        "transfer_data": false,
    });

    let (before, _) = get_json(&app, "/plugins").await;
    let (draft_status, draft_body) = post_json(&app, "/setup/draft", body.clone()).await;
    let (apply_status, apply_body) = post_json(&app, "/setup/apply", body.clone()).await;
    let (after, _) = get_json(&app, "/plugins").await;
    let (again, _) = post_json(&app, "/setup/apply", body).await;
    let config_written = data_dir.join("config.toml").exists();
    let setup_required = state.inner.read().await.setup_required;

    for (name, value) in saved {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
    let _ = std::fs::remove_dir_all(&data_dir);

    assert_eq!(before, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(draft_status, StatusCode::OK, "{draft_body}");
    assert_eq!(draft_body["errors"], serde_json::json!({}));
    assert_eq!(apply_status, StatusCode::OK, "{apply_body}");
    assert_eq!(apply_body["saved"], true);
    assert_eq!(apply_body["restart_required"], false);
    assert!(config_written);
    assert!(!setup_required);
    assert_eq!(after, StatusCode::OK);
    assert_eq!(again, StatusCode::CONFLICT);
    assert!(!apply_body.to_string().contains("\"x\""));
}
