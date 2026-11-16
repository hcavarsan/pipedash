use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{
    Method,
    Request,
};
use pipedash_core::domain::{
    Pipeline,
    PipelineStatus,
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
use crate::routes::CONFIG_ENV;
use crate::ws::event_bus::WebSocketEventBus;

struct Harness {
    app: Router,
    pipeline_ids: Vec<String>,
    data_dir: PathBuf,
    _env: tokio::sync::MutexGuard<'static, ()>,
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.data_dir);
    }
}

fn pipeline(provider_id: i64, name: &str) -> Pipeline {
    Pipeline {
        id: format!("stub__{provider_id}__{name}"),
        provider_id,
        provider_type: "stub".into(),
        name: name.into(),
        status: PipelineStatus::Success,
        last_run: None,
        last_updated: chrono::Utc::now(),
        repository: format!("acme/{name}"),
        branch: None,
        workflow_file: None,
        resource_state: None,
        metadata: Default::default(),
        overdue_run: None,
        typical_duration_seconds: None,
        actions: Default::default(),
        correlation: Default::default(),
    }
}

async fn harness(name: &str) -> Harness {
    let env = CONFIG_ENV.lock().await;
    let data_dir =
        std::env::temp_dir().join(format!("pipedash-apps-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("config.toml"), "").unwrap();
    std::env::set_var("PIPEDASH_CONFIG_PATH", data_dir.join("config.toml"));

    let mut config = PipedashConfig::default();
    config.storage.backend = StorageBackendType::Sqlite;
    config.storage.data_dir = data_dir.display().to_string();
    config.storage.vault_password = Some(format!("vault-{}", std::process::id()));
    let storage_manager = StorageManager::from_config(config, false).await.unwrap();
    let provider_id = storage_manager
        .config_backend()
        .create_provider(&ProviderConfig {
            id: None,
            name: "stub".into(),
            provider_type: "stub".into(),
            token: String::new(),
            config: [
                ("server_url".to_string(), "http://127.0.0.1:9".to_string()),
                ("username".to_string(), "admin".to_string()),
            ]
            .into(),
            refresh_interval: 30,
            version: None,
        })
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
    let pipelines = vec![pipeline(provider_id, "api"), pipeline(provider_id, "web")];
    core.provider_service
        .repository()
        .update_pipelines_cache(provider_id, &pipelines)
        .await
        .unwrap();
    let state = AppState::initialized(
        ws_event_bus,
        core,
        storage_manager,
        crate::test_support::registry(),
    );
    let app = Router::new()
        .nest("/applications", router())
        .with_state(state);

    Harness {
        app,
        pipeline_ids: pipelines.into_iter().map(|p| p.id).collect(),
        data_dir,
        _env: env,
    }
}

async fn call(app: &Router, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    let body = match body {
        Some(value) => {
            builder = builder.header("content-type", "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let response = app
        .clone()
        .oneshot(builder.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();

    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn applications_list_decodes_comma_joined_filters() {
    let h = harness("list").await;

    let (status, created) = call(
        &h.app,
        Method::POST,
        "/applications",
        Some(json!({ "name": "Storefront", "members": [{ "pipeline_id": h.pipeline_ids[0] }] })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["summary"]["id"].as_i64().unwrap();

    for query in [
        "",
        "search=store",
        "health=failed,degraded,success&provider_ids=1,2&behind_only=false",
        "include_hidden=true&with_suggestions=false&sort=health&direction=desc",
    ] {
        let (status, list) =
            call(&h.app, Method::GET, &format!("/applications?{query}"), None).await;
        assert_eq!(status, StatusCode::OK, "{query}");
        assert!(list["applications"].is_array(), "{query}");
    }

    let (_, list) = call(&h.app, Method::GET, "/applications?search=store", None).await;
    assert_eq!(list["applications"][0]["id"], id);
    let (_, list) = call(&h.app, Method::GET, "/applications?search=zzz", None).await;
    assert_eq!(list["applications"].as_array().unwrap().len(), 0);

    for query in ["health=failed,bogus", "provider_ids=1,x", "sort=bogus"] {
        let (status, _) = call(&h.app, Method::GET, &format!("/applications?{query}"), None).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{query}");
    }
}

#[tokio::test]
async fn unknown_application_is_not_found_on_every_id_route() {
    let h = harness("unknown").await;

    for (method, uri, body) in [
        (Method::GET, "/applications/999", None),
        (Method::GET, "/applications/999/changes?limit=10", None),
        (Method::DELETE, "/applications/999", None),
        (Method::DELETE, "/applications/999/overrides/1", None),
        (
            Method::POST,
            "/applications/999/edits",
            Some(json!({ "type": "rename", "name": "Nope" })),
        ),
    ] {
        let (status, error) = call(&h.app, method.clone(), uri, body).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{method} {uri}");
        assert_eq!(error["code"], "NOT_FOUND", "{method} {uri}");
    }
}

#[tokio::test]
async fn application_create_edit_and_delete_over_http() {
    let h = harness("create").await;

    let (status, created) = call(
        &h.app,
        Method::POST,
        "/applications",
        Some(json!({ "name": "Storefront", "members": [{ "pipeline_id": h.pipeline_ids[0] }] })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["summary"]["id"].as_i64().unwrap();
    assert_eq!(created["summary"]["name"], "Storefront");

    let (status, renamed) = call(
        &h.app,
        Method::POST,
        &format!("/applications/{id}/edits"),
        Some(json!({ "type": "rename", "name": "Shop" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(renamed["summary"]["name"], "Shop");

    let (status, detail) = call(&h.app, Method::GET, &format!("/applications/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["summary"]["name"], "Shop");

    let (status, _) = call(&h.app, Method::DELETE, &format!("/applications/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(&h.app, Method::GET, &format!("/applications/{id}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn literal_application_routes_are_not_captured_by_the_id_route() {
    let h = harness("literal").await;

    let (status, suggestions) = call(&h.app, Method::GET, "/applications/suggestions", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(suggestions.is_array());

    let (status, _) = call(
        &h.app,
        Method::POST,
        "/applications/suggestions/accept",
        Some(json!({ "ids": [] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, applications) = call(
        &h.app,
        Method::GET,
        &format!("/applications/by-pipeline/{}", h.pipeline_ids[0]),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(applications.is_array());

    for path in ["commit-locations", "deploy-changes"] {
        let uri = format!(
            "/applications/{path}?pipeline_id={}&run_number=1",
            h.pipeline_ids[0]
        );
        let (status, _) = call(&h.app, Method::GET, &uri, None).await;
        assert_ne!(status, StatusCode::BAD_REQUEST, "{path}");
        assert_ne!(status, StatusCode::METHOD_NOT_ALLOWED, "{path}");
    }
}
