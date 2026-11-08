use std::sync::Arc;

use axum::body::Body;
use axum::http::header;
use pipedash_core::infrastructure::{
    PipedashConfig,
    StorageBackendType,
    StorageManager,
};
use pipedash_core::ProviderConfig;
use tower::ServiceExt;

use super::*;
use crate::ws::event_bus::WebSocketEventBus;

const TOKEN: &str = "operator-secret";

fn app_with_token() -> Router {
    let state = AppState::config_error(
        Arc::new(WebSocketEventBus::new()),
        "test".into(),
        crate::test_support::registry(),
    );
    state.set_api_token(TOKEN);
    app(state, None)
}

async fn send(app: &Router, method: Method, uri: &str, auth: Option<&str>) -> StatusCode {
    let mut request = axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = auth {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let body = if uri.ends_with("/unlock") {
        Body::from(r#"{"password":"wrong-password"}"#)
    } else {
        Body::empty()
    };

    app.clone()
        .oneshot(request.body(body).unwrap())
        .await
        .unwrap()
        .status()
}

async fn assert_api_still_requires_token(app: &Router) {
    assert_eq!(
        send(app, Method::GET, "/api/v1/providers", None).await,
        StatusCode::UNAUTHORIZED
    );
    assert_ne!(
        send(app, Method::GET, "/api/v1/providers", Some(TOKEN)).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn vault_status_is_public_but_lock_and_unlock_require_token() {
    let app = app_with_token();

    assert_eq!(
        send(&app, Method::GET, "/api/v1/vault/status", None).await,
        StatusCode::OK
    );
    for uri in ["/api/v1/vault/lock", "/api/v1/vault/unlock"] {
        assert_eq!(
            send(&app, Method::POST, uri, None).await,
            StatusCode::UNAUTHORIZED,
            "{uri}"
        );
        assert_eq!(
            send(&app, Method::POST, uri, Some("wrong")).await,
            StatusCode::UNAUTHORIZED,
            "{uri}"
        );
    }
}

#[tokio::test]
async fn paired_device_token_is_accepted_and_unknown_token_is_rejected() {
    let host = test_support::hosted("lib-paired-token").await;
    let (_, token) = host
        .state
        .core()
        .await
        .unwrap()
        .pair_device("Pixel", "android")
        .await
        .unwrap();
    let app = app(host.state.clone(), None);

    assert_eq!(
        send(&app, Method::GET, "/api/v1/devices", Some(&token)).await,
        StatusCode::OK
    );
    for auth in [
        None,
        Some("unknown-token"),
        Some(token.to_uppercase().as_str()),
    ] {
        assert_eq!(
            send(&app, Method::GET, "/api/v1/devices", auth).await,
            StatusCode::UNAUTHORIZED,
            "{auth:?}"
        );
    }
}

async fn get_json(app: &Router, uri: &str, auth: Option<&str>) -> (StatusCode, serde_json::Value) {
    let mut request = axum::http::Request::builder().uri(uri);
    if let Some(token) = auth {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    (status, serde_json::from_slice(&body).unwrap_or_default())
}

#[tokio::test]
async fn a_device_token_is_resolved_on_a_server_without_an_api_token() {
    let host = test_support::server("lib-open-device").await;
    assert!(!host.state.requires_auth());
    let (device, token) = host
        .state
        .core()
        .await
        .unwrap()
        .pair_device("Pixel", "android")
        .await
        .unwrap();
    let app = app(host.state.clone(), None);

    let (status, body) = get_json(&app, "/api/v1/devices", Some(&token)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["devices"][0]["id"], device.id);
    assert_eq!(body["devices"][0]["current"], true);

    let (status, body) = get_json(&app, "/api/v1/storage/config", Some(&token)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "HOST_MANAGED");

    for auth in [None, Some("unknown-token")] {
        let (status, body) = get_json(&app, "/api/v1/devices", auth).await;
        assert_eq!(status, StatusCode::OK, "{auth:?}");
        assert_eq!(body["devices"][0]["current"], false, "{auth:?}");
        assert_ne!(
            send(&app, Method::GET, "/api/v1/storage/config", auth).await,
            StatusCode::CONFLICT,
            "{auth:?}"
        );
    }
}

#[tokio::test]
async fn a_failed_device_lookup_is_a_retryable_503_not_a_401() {
    let host = test_support::hosted("lib-lookup-failure").await;
    let core = host.state.core().await.unwrap();
    let (_, token) = core.pair_device("Pixel", "android").await.unwrap();
    let app = app(host.state.clone(), None);
    let pipedash_core::infrastructure::database::DatabasePool::Sqlite(pool) =
        core.provider_service.repository().cache_pool().clone()
    else {
        panic!("expected a SQLite pool");
    };
    pool.close().await;

    let (status, body) = get_json(&app, "/api/v1/providers", Some(&token)).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["code"], "SERVER_NOT_READY");
}

#[tokio::test]
async fn a_hosted_state_without_an_api_token_still_requires_a_token() {
    let host = test_support::hosted("lib-hosted-auth").await;
    assert!(host.state.api_token().is_none());
    let app = app(host.state.clone(), None);

    assert_eq!(
        send(&app, Method::GET, "/api/v1/providers", None).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(&app, Method::GET, "/api/v1/vault/status", None).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_paired_device_cannot_manage_a_server_host() {
    let host = test_support::hosted("lib-server-guard").await;
    let (_, token) = host
        .state
        .core()
        .await
        .unwrap()
        .pair_device("Pixel", "android")
        .await
        .unwrap();
    let mut state = host.state.clone();
    state.desktop_host = None;
    state.set_api_token(TOKEN);
    let app = app(state, None);

    assert_eq!(
        send(&app, Method::GET, "/api/v1/providers", Some(&token)).await,
        StatusCode::OK
    );
    for (method, uri) in [
        (Method::POST, "/api/v1/factory-reset"),
        (Method::POST, "/api/v1/vault/lock"),
        (Method::GET, "/api/v1/storage/config"),
    ] {
        assert_eq!(
            send(&app, method, uri, Some(&token)).await,
            StatusCode::CONFLICT,
            "{uri}"
        );
    }
    assert_ne!(
        send(&app, Method::GET, "/api/v1/storage/config", Some(TOKEN)).await,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn a_hosted_state_refuses_host_managed_routes_and_reports_a_locked_core() {
    let host = test_support::hosted("lib-hosted-guard").await;
    let (_, token) = host
        .state
        .core()
        .await
        .unwrap()
        .pair_device("Pixel", "android")
        .await
        .unwrap();
    let app = app(host.state.clone(), None);

    for (method, uri) in [
        (Method::POST, "/api/v1/factory-reset"),
        (Method::POST, "/api/v1/vault/lock"),
        (Method::GET, "/api/v1/storage/config"),
        (Method::GET, "/api/v1/push/subscriptions"),
    ] {
        assert_eq!(
            send(&app, method, uri, Some(&token)).await,
            StatusCode::CONFLICT,
            "{uri}"
        );
    }
    assert_ne!(
        send(&app, Method::GET, "/api/v1/providers", Some(&token)).await,
        StatusCode::CONFLICT
    );

    let core = host.state.core().await.unwrap();
    assert_eq!(
        send(&app, Method::GET, "/api/v1/downloads/abc", None).await,
        StatusCode::NOT_FOUND
    );

    host.state.set_core(None).await;
    assert_eq!(
        send(&app, Method::GET, "/api/v1/providers", Some(&token)).await,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        send(&app, Method::GET, "/api/v1/downloads/abc", None).await,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        send(&app, Method::GET, "/api/v1/vault/status", None).await,
        StatusCode::OK
    );

    host.state.set_core(Some(core)).await;
    assert_eq!(
        send(&app, Method::GET, "/api/v1/downloads/abc", None).await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn vault_lock_keeps_api_auth() {
    let app = app_with_token();

    send(&app, Method::POST, "/api/v1/vault/lock", None).await;
    assert_api_still_requires_token(&app).await;

    assert_eq!(
        send(&app, Method::POST, "/api/v1/vault/lock", Some(TOKEN)).await,
        StatusCode::OK
    );
    assert_api_still_requires_token(&app).await;
}

async fn locked_sqlite_vault(data_dir: &std::path::Path) -> StorageManager {
    let mut config = PipedashConfig::default();
    config.storage.backend = StorageBackendType::Sqlite;
    config.storage.data_dir = data_dir.display().to_string();

    let mut seeded = config.clone();
    seeded.storage.vault_password = Some("vault-password".into());
    let manager = StorageManager::from_config(seeded, false).await.unwrap();
    let provider = ProviderConfig {
        id: None,
        name: "stub".into(),
        provider_type: "stub".into(),
        token: String::new(),
        config: Default::default(),
        refresh_interval: 30,
        version: None,
    };
    let id = manager
        .config_backend()
        .create_provider(&provider)
        .await
        .unwrap();
    manager
        .token_store()
        .await
        .store_token(id, "ghp_secret")
        .await
        .unwrap();

    let locked = StorageManager::from_config_allow_locked(config, false)
        .await
        .unwrap();
    assert!(locked.is_vault_locked().await);
    locked
}

#[tokio::test]
async fn failed_vault_unlock_keeps_api_auth() {
    let data_dir =
        std::env::temp_dir().join(format!("pipedash-failed-unlock-{}", std::process::id()));
    let state = AppState::config_error(
        Arc::new(WebSocketEventBus::new()),
        "test".into(),
        crate::test_support::registry(),
    );
    state.inner.write().await.storage_manager =
        Some(Arc::new(locked_sqlite_vault(&data_dir).await));
    state.set_api_token(TOKEN);
    let app = app(state.clone(), None);

    let response = app
        .clone()
        .oneshot(
            axum::http::Request::post("/api/v1/vault/unlock")
                .header(header::CONTENT_TYPE, "application/json")
                .header(AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::from(r#"{"password":"wrong-password"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["success"], false);
    assert_eq!(state.api_token().as_deref(), Some(TOKEN));
    assert_api_still_requires_token(&app).await;
    let _ = std::fs::remove_dir_all(&data_dir);
}

#[tokio::test]
async fn empty_vault_unlock_cannot_replace_the_api_token() {
    let data_dir =
        std::env::temp_dir().join(format!("pipedash-empty-vault-{}", std::process::id()));
    let mut config = PipedashConfig::default();
    config.storage.backend = StorageBackendType::Sqlite;
    config.storage.data_dir = data_dir.display().to_string();
    config.storage.vault_password = Some(TOKEN.into());
    let state = AppState::config_error(
        Arc::new(WebSocketEventBus::new()),
        "test".into(),
        crate::test_support::registry(),
    );
    state.inner.write().await.storage_manager = Some(Arc::new(
        StorageManager::from_config(config, false).await.unwrap(),
    ));
    state.set_api_token(TOKEN);
    let app = app(state.clone(), None);

    for uri in ["/api/v1/vault/lock", "/api/v1/vault/unlock"] {
        assert_eq!(
            send(&app, Method::POST, uri, None).await,
            StatusCode::UNAUTHORIZED,
            "{uri}"
        );
    }
    send(&app, Method::POST, "/api/v1/vault/unlock", Some(TOKEN)).await;

    assert_eq!(state.api_token().as_deref(), Some(TOKEN));
    assert_eq!(
        send(
            &app,
            Method::GET,
            "/api/v1/providers",
            Some("wrong-password")
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
    assert_api_still_requires_token(&app).await;
    let _ = std::fs::remove_dir_all(&data_dir);
}

#[test]
fn bearer_matches_accepts_exact_token() {
    assert!(bearer_matches(
        Some("Bearer operator-secret"),
        "operator-secret"
    ));
}

#[test]
fn bearer_matches_rejects_missing_wrong_and_partial_tokens() {
    for auth_header in [
        None,
        Some("operator-secret"),
        Some("Bearer "),
        Some("Bearer wrong"),
        Some("Bearer operator"),
        Some("Bearer operator-secret-extra"),
    ] {
        assert!(
            !bearer_matches(auth_header, "operator-secret"),
            "accepted {auth_header:?}"
        );
    }
}

#[tokio::test]
async fn only_get_downloads_skip_the_token() {
    let app = app_with_token();

    assert_eq!(
        send(
            &app,
            Method::POST,
            "/api/v1/pipelines/p/runs/1/logs/n/download-ticket",
            None
        )
        .await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(&app, Method::GET, "/api/v1/downloads/abc", None).await,
        StatusCode::NOT_FOUND
    );
    for method in [Method::POST, Method::DELETE] {
        assert_eq!(
            send(&app, method, "/api/v1/downloads/abc", None).await,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        send(&app, Method::GET, "/api/v1/pipelines/p/runs/1/logs", None).await,
        StatusCode::UNAUTHORIZED
    );
}
