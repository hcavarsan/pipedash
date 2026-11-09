use std::sync::Arc;

use axum::body::Body;
use axum::http::{
    header,
    Request,
    StatusCode,
};
use pipedash_core::infrastructure::StorageManager;
use tower::ServiceExt;

use super::*;
use crate::ws::event_bus::WebSocketEventBus;

async fn call(request: Request<Body>) -> (StatusCode, serde_json::Value) {
    let app = router().with_state(AppState::setup_mode(
        Arc::new(WebSocketEventBus::new()),
        crate::test_support::registry(),
    ));
    let response = app.oneshot(request).await.unwrap();
    let status = response.status();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    (status, serde_json::from_slice(&body).unwrap())
}

fn post_config(uri: &str, data_dir: &std::path::Path) -> Request<Body> {
    let mut config = PipedashConfig::default();
    config.storage.data_dir = data_dir.display().to_string();

    Request::post(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&config).unwrap()))
        .unwrap()
}

#[tokio::test]
async fn editor_preserves_environment_references_when_saving() {
    const CHILD: &str = "PIPEDASH_EDITOR_ENV_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let root = std::env::temp_dir().join(format!("pipedash-editor-env-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "routes::storage::tests::editor_preserves_environment_references_when_saving",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("PIPEDASH_CONFIG_PATH", root.join("config.toml"))
            .env("PIPEDASH_EDITOR_TEST_BACKEND", "sqlite")
            .env("PIPEDASH_EDITOR_TEST_SECRET", "editor-secret-value")
            .output()
            .unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let config_path = ConfigLoader::discover_config_path();
    let content = format!(
        "[general]\nstatus_pages_enabled = false\n[storage]\nbackend = \"${{PIPEDASH_EDITOR_TEST_BACKEND}}\"\ndata_dir = {:?}\n",
        config_path.parent().unwrap().to_string_lossy()
    );
    std::fs::write(&config_path, &content).unwrap();
    let config = ConfigLoader::load(&config_path).unwrap();
    let manager = StorageManager::with_token_store(
        config,
        Arc::new(pipedash_core::infrastructure::token_store::MemoryTokenStore::new()),
        true,
    )
    .await
    .unwrap();
    let state = AppState::setup_mode(
        Arc::new(WebSocketEventBus::new()),
        crate::test_support::registry(),
    );
    state.inner.write().await.storage_manager = Some(Arc::new(manager));
    let app = router().with_state(state);
    let edited = content.replace(
        "status_pages_enabled = false",
        "status_pages_enabled = true",
    );

    let mut statuses = Vec::new();
    for (method, uri, body) in [
        (
            "POST",
            "/config/analyze",
            serde_json::json!({"new_content": edited}),
        ),
        (
            "PUT",
            "/config/content",
            serde_json::json!({"content": edited}),
        ),
    ] {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        statuses.push(app.clone().oneshot(request).await.unwrap().status());
    }

    assert_eq!(statuses, [StatusCode::OK, StatusCode::OK]);
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), edited);
    let saved = ConfigLoader::load(&config_path).unwrap();
    assert_eq!(
        saved.storage.backend,
        pipedash_core::infrastructure::config::StorageBackend::Sqlite
    );
    assert!(saved.general.status_pages_enabled);

    let invalid = "[storage]\nbackend = \"${PIPEDASH_EDITOR_TEST_SECRET}\"\n";
    for (method, uri, body) in [
        (
            "POST",
            "/config/analyze",
            serde_json::json!({"new_content": invalid}),
        ),
        (
            "PUT",
            "/config/content",
            serde_json::json!({"content": invalid}),
        ),
    ] {
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(!String::from_utf8_lossy(&body).contains("editor-secret-value"));
    }
    assert_eq!(std::fs::read_to_string(&config_path).unwrap(), edited);
}

#[tokio::test]
async fn test_connection_checks_both_backends_of_the_posted_config() {
    let data_dir = std::env::temp_dir().join(format!(
        "pipedash-storage-connection-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&data_dir).unwrap();

    let (status, body) = call(post_config("/test-connection", &data_dir)).await;
    let _ = std::fs::remove_dir_all(&data_dir);

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        serde_json::json!({
            "success": true,
            "message": "All storage backends are accessible"
        })
    );
}

#[tokio::test]
async fn default_data_dir_returns_a_json_string() {
    let request = Request::get("/default-data-dir")
        .body(Body::empty())
        .unwrap();

    let (status, body) = call(request).await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        serde_json::json!(PipedashConfig::default_data_dir().display().to_string())
    );
}

async fn post_draft(
    app: &Router, path: &str, body: &serde_json::Value, token: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut request = Request::post(path).header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

#[tokio::test]
async fn configured_storage_wizard_moves_data_without_reopening_initial_setup() {
    const CHILD: &str = "PIPEDASH_STORAGE_WIZARD_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let root =
            std::env::temp_dir().join(format!("pipedash-storage-wizard-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "routes::storage::tests::configured_storage_wizard_moves_data_without_reopening_initial_setup",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("PIPEDASH_CONFIG_PATH", root.join("config.toml"))
            .env("PIPEDASH_VAULT_PASSWORD", "operator-password")
            .env_remove("PIPEDASH_DATA_DIR")
            .output()
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let _ = rustls::crypto::ring::default_provider().install_default();
    let config_path = ConfigLoader::discover_config_path();
    let root = config_path.parent().unwrap();
    let target = root.join("target");
    let mut config = PipedashConfig::default();
    config.storage.data_dir = root.join("source").display().to_string();
    config.general.metrics_enabled = false;
    config.general.status_pages_enabled = false;
    config.providers.insert(
        "retained".into(),
        pipedash_core::infrastructure::config::schema::ProviderFileConfig {
            provider_type: "stub".into(),
            token: "retained-token".into(),
            ..Default::default()
        },
    );
    ConfigLoader::save(&config, &config_path).unwrap();
    let manager = StorageManager::from_config(config, false).await.unwrap();
    let bus = Arc::new(WebSocketEventBus::new());
    let registry = crate::test_support::registry();
    let core =
        pipedash_core::CoreContext::with_storage_manager(&manager, bus.clone(), registry.clone())
            .await
            .unwrap();
    let provider_id = manager.config_backend().list_providers().await.unwrap()[0]
        .id
        .unwrap();
    assert_eq!(
        core.token_store.get_token(provider_id).await.unwrap(),
        "retained-token"
    );
    let state = AppState::initialized(bus, core, manager, registry);
    state.set_api_token("operator-password");
    let app = crate::app(state.clone(), None);
    let body = serde_json::json!({
        "backend": "sqlite",
        "data_dir": target.display().to_string(),
        "postgres_url": "",
        "vault_password": "",
        "vault_password_confirm": "",
        "transfer_data": true,
    });

    for path in ["/api/v1/storage/draft", "/api/v1/storage/apply"] {
        for token in [None, Some("wrong-password")] {
            assert_eq!(
                post_draft(&app, path, &body, token).await.0,
                StatusCode::UNAUTHORIZED
            );
        }
    }
    let (status, prepared) = post_draft(
        &app,
        "/api/v1/storage/draft",
        &body,
        Some("operator-password"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{prepared}");
    assert_eq!(prepared["errors"], serde_json::json!({}));
    assert_eq!(prepared["needs_migration"], true);
    let (status, applied) = post_draft(
        &app,
        "/api/v1/storage/apply",
        &body,
        Some("operator-password"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    assert_eq!(applied["saved"], true);
    assert_eq!(applied["migration_stats"]["tokens_migrated"], 1);
    let saved = ConfigLoader::load(&config_path).unwrap();
    assert_eq!(saved.data_dir(), target);
    let manager = state.storage_manager().await.unwrap();
    assert_eq!(manager.config().data_dir(), target);
    let provider_id = manager.config_backend().list_providers().await.unwrap()[0]
        .id
        .unwrap();
    assert_eq!(
        state
            .core()
            .await
            .unwrap()
            .token_store
            .get_token(provider_id)
            .await
            .unwrap(),
        "retained-token"
    );
    assert_eq!(state.api_token().as_deref(), Some("operator-password"));
    for path in ["/api/v1/setup/draft", "/api/v1/setup/apply"] {
        assert_eq!(
            post_draft(&app, path, &body, Some("operator-password"))
                .await
                .0,
            StatusCode::CONFLICT
        );
    }
    state.core().await.unwrap().shutdown().await;
}
