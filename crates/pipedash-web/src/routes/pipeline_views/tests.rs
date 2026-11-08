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
    }
}

async fn harness(name: &str) -> Harness {
    let env = CONFIG_ENV.lock().await;
    let data_dir =
        std::env::temp_dir().join(format!("pipedash-views-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("config.toml"), "").unwrap();
    std::env::set_var("PIPEDASH_CONFIG_PATH", data_dir.join("config.toml"));

    let mut config = PipedashConfig::default();
    config.storage.backend = StorageBackendType::Sqlite;
    config.storage.data_dir = data_dir.display().to_string();
    config.storage.vault_password = Some("vault-password".into());
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
        .nest("/pipeline-views", router())
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
async fn pipeline_views_create_add_and_view_over_http() {
    let h = harness("create").await;

    let (status, view) = call(
        &h.app,
        Method::POST,
        "/pipeline-views",
        Some(json!({ "name": "Release" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = view["id"].as_str().unwrap().to_string();
    assert!(id.parse::<i64>().is_ok());

    let (status, _) = call(
        &h.app,
        Method::PUT,
        &format!("/pipeline-views/{id}/membership"),
        Some(json!({ "pipeline_id": h.pipeline_ids[0], "member": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, page) = call(
        &h.app,
        Method::GET,
        &format!("/pipeline-views/page?view={id}&show_hidden=false"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total"], 1);

    let (status, child) = call(
        &h.app,
        Method::POST,
        "/pipeline-views",
        Some(json!({ "name": "Child", "parent": id })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let child_id = child["id"].as_str().unwrap().to_string();

    let (status, _) = call(
        &h.app,
        Method::PUT,
        &format!("/pipeline-views/{child_id}/membership"),
        Some(json!({ "pipeline_id": h.pipeline_ids[1], "member": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, page) = call(
        &h.app,
        Method::GET,
        &format!("/pipeline-views/page?view={id}&show_hidden=false"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["total"], 2);
    assert_eq!(page["sections"][0]["id"], format!("view:{child_id}"));
    assert_eq!(page["sections"][0]["kind"], "view");
    assert_eq!(page["sections"][1]["kind"], "repository");

    let (status, error) = call(&h.app, Method::GET, "/pipeline-views/page?view=999", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(error["code"], "NOT_FOUND");
}

#[tokio::test]
async fn pipeline_view_page_filters_by_any_of_several_statuses() {
    let h = harness("statuses").await;

    for (statuses, total) in [
        ("failed,success", h.pipeline_ids.len()),
        ("success", h.pipeline_ids.len()),
        ("running,pending", 0),
    ] {
        let (status, page) = call(
            &h.app,
            Method::GET,
            &format!("/pipeline-views/page?view=all&statuses={statuses}"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{statuses}");
        assert_eq!(page["total"], total, "{statuses}");
    }

    let (status, _) = call(
        &h.app,
        Method::GET,
        "/pipeline-views/page?view=all&statuses=bogus",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn pipeline_view_pins_round_trip_over_http() {
    let h = harness("pins").await;

    let (status, _) = call(
        &h.app,
        Method::PUT,
        "/pipeline-views/pins",
        Some(json!({ "kind": "repository", "ref": "acme/api", "pinned": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, pins) = call(&h.app, Method::GET, "/pipeline-views/pins", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        pins,
        json!([{ "kind": "repository", "ref": "acme/api", "name": "api" }])
    );

    let (status, page) = call(&h.app, Method::GET, "/pipeline-views/page?view=all", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["pinned_blocks"][0]["ref"], "acme/api");
    assert_eq!(
        page["pinned_blocks"][0]["pipelines"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let (status, error) = call(
        &h.app,
        Method::PUT,
        "/pipeline-views/pins",
        Some(json!({ "kind": "repository", "ref": "acme/missing", "pinned": true })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(error["code"], "NOT_FOUND");

    let (status, _) = call(
        &h.app,
        Method::PUT,
        "/pipeline-views/pins",
        Some(json!({ "kind": "repository", "ref": "acme/api", "pinned": false })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, pins) = call(&h.app, Method::GET, "/pipeline-views/pins", None).await;
    assert_eq!(pins, json!([]));
}

#[tokio::test]
async fn pipeline_view_menu_returns_the_snapshot_the_desktop_tray_uses() {
    let h = harness("menu").await;

    let (status, empty) = call(&h.app, Method::GET, "/pipeline-views/menu", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(empty, json!({ "pipelines": [], "shortcuts": [] }));

    let (status, _) = call(
        &h.app,
        Method::PUT,
        "/pipeline-views/pinned/membership",
        Some(json!({ "pipeline_id": h.pipeline_ids[0], "member": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = call(
        &h.app,
        Method::PUT,
        "/pipeline-views/pins",
        Some(json!({ "kind": "repository", "ref": "acme/web", "pinned": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, menu) = call(&h.app, Method::GET, "/pipeline-views/menu", None).await;
    assert_eq!(status, StatusCode::OK);
    let snapshot: PipelineMenuSnapshot = serde_json::from_value(menu.clone()).unwrap();
    assert_eq!(snapshot.pipelines.len(), 1);
    assert_eq!(snapshot.pipelines[0].pipeline_id, h.pipeline_ids[0]);
    assert_eq!(
        snapshot.pipelines[0]
            .pipeline
            .as_ref()
            .map(|pipeline| pipeline.name.as_str()),
        Some("api")
    );
    assert_eq!(snapshot.shortcuts[0].reference, "acme/web");
    assert_eq!(menu["pipelines"][0]["pipeline_id"], h.pipeline_ids[0]);

    let (status, _) = call(
        &h.app,
        Method::PUT,
        "/pipeline-views/pinned/membership",
        Some(json!({ "pipeline_id": h.pipeline_ids[0], "member": false })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, menu) = call(&h.app, Method::GET, "/pipeline-views/menu", None).await;
    assert_eq!(menu["pipelines"], json!([]));
}

#[tokio::test]
async fn pipeline_view_batch_membership_selection_options_and_navigation_over_http() {
    let h = harness("batch").await;
    let (_, view) = call(
        &h.app,
        Method::POST,
        "/pipeline-views",
        Some(json!({ "name": "Release" })),
    )
    .await;
    let id = view["id"].as_str().unwrap().to_string();
    let selected = |entries: &Value| {
        entries
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["view"]["id"] == id.as_str())
            .map(|e| e["selected_count"].as_u64().unwrap())
    };

    let (status, entries) = call(
        &h.app,
        Method::PUT,
        &format!("/pipeline-views/{id}/memberships"),
        Some(json!({ "pipeline_ids": [h.pipeline_ids[0], h.pipeline_ids[1]], "member": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(selected(&entries), Some(2));

    let (status, entries) = call(
        &h.app,
        Method::POST,
        "/pipeline-views/selection",
        Some(json!({ "pipeline_ids": [h.pipeline_ids[0]] })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(selected(&entries), Some(1));

    let (status, options) = call(&h.app, Method::GET, "/pipeline-views/editor-options", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(options["eligible_parents"][0]["id"], id.as_str());
    let (status, options) = call(
        &h.app,
        Method::GET,
        &format!("/pipeline-views/editor-options?view={id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(options["eligible_parents"], json!([]));
    assert_eq!(options["can_add_sub_view"], true);

    let (status, nav) = call(
        &h.app,
        Method::GET,
        "/pipeline-views/navigation?search=rele",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(nav["views"][0]["id"], id.as_str());
    assert_eq!(nav["views"].as_array().unwrap().len(), 1);
}
