use std::sync::Arc;

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
use pipedash_web::ws::event_bus::WebSocketEventBus;
use serde_json::{
    json,
    Value,
};
use tauri::ipc::{
    CallbackFn,
    InvokeBody,
};
use tauri::test::{
    get_ipc_response,
    mock_builder,
    mock_context,
    noop_assets,
    MockRuntime,
    INVOKE_KEY,
};
use tauri::webview::InvokeRequest;
use tauri::{
    App,
    WebviewWindow,
    WebviewWindowBuilder,
};
use tokio::sync::{
    broadcast,
    RwLock,
};

use crate::MaybeCoreContext;

static CONFIG_ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Ipc {
    webview: WebviewWindow<MockRuntime>,
    pipeline_ids: Vec<String>,
    _app: App<MockRuntime>,
    _storage: StorageManager,
    _dir: tempfile::TempDir,
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

async fn core(dir: &std::path::Path) -> (StorageManager, CoreContext, Vec<String>) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    std::fs::write(
        dir.join("config.toml"),
        format!("[storage]\ndata_dir = {:?}\n", dir.display().to_string()),
    )
    .unwrap();
    let _env = CONFIG_ENV.lock().await;
    std::env::set_var("PIPEDASH_CONFIG_PATH", dir.join("config.toml"));
    let mut config = PipedashConfig::default();
    config.storage.backend = StorageBackendType::Sqlite;
    config.storage.data_dir = dir.display().to_string();
    config.storage.vault_password = Some(format!("vault-{}", std::process::id()));
    let storage = StorageManager::from_config(config, false).await.unwrap();
    let bus = Arc::new(WebSocketEventBus::from_sender(broadcast::channel(64).0));
    let core =
        CoreContext::with_storage_manager(&storage, bus, crate::plugin_registry::plugin_registry())
            .await
            .unwrap();
    let provider_id = storage
        .config_backend()
        .create_provider(&ProviderConfig {
            id: None,
            name: "stub".into(),
            provider_type: "stub".into(),
            token: String::new(),
            config: Default::default(),
            refresh_interval: 30,
            version: None,
        })
        .await
        .unwrap();
    let pipelines = vec![pipeline(provider_id, "api"), pipeline(provider_id, "web")];
    core.provider_service
        .repository()
        .update_pipelines_cache(provider_id, &pipelines)
        .await
        .unwrap();
    (storage, core, pipelines.into_iter().map(|p| p.id).collect())
}

fn ipc() -> Ipc {
    let dir = tempfile::tempdir().unwrap();
    let (storage, core, pipeline_ids) = tauri::async_runtime::block_on(core(dir.path()));
    let app = mock_builder()
        .manage(MaybeCoreContext(Arc::new(RwLock::new(Some(Arc::new(
            core,
        ))))))
        .invoke_handler(tauri::generate_handler![
            crate::commands::applications::list_applications,
            crate::commands::applications::get_application,
            crate::commands::applications::get_application_changes,
            crate::commands::applications::get_application_metrics,
            crate::commands::applications::create_application,
            crate::commands::applications::edit_application,
            crate::commands::applications::delete_application,
            crate::commands::applications::revert_application_override,
            crate::commands::applications::list_application_suggestions,
            crate::commands::applications::accept_application_suggestions,
            crate::commands::applications::get_pipeline_applications,
            crate::commands::applications::get_commit_locations,
            crate::commands::applications::get_deploy_changes,
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    let webview = WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    Ipc {
        webview,
        pipeline_ids,
        _app: app,
        _storage: storage,
        _dir: dir,
    }
}

impl Ipc {
    fn raw(&self, cmd: &str, body: Value) -> Result<Value, Value> {
        get_ipc_response(
            &self.webview,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: if cfg!(any(windows, target_os = "android")) {
                    "http://tauri.localhost"
                } else {
                    "tauri://localhost"
                }
                .parse()
                .unwrap(),
                body: InvokeBody::Json(body),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|response| response.deserialize::<Value>().unwrap())
    }

    fn invoke(&self, cmd: &str, body: Value) -> Result<Value, Value> {
        let result = self.raw(cmd, body);
        if let Err(error) = &result {
            assert!(
                error.get("error").is_some(),
                "{cmd}: arguments were not decoded: {error}"
            );
        }
        result
    }

    fn ok(&self, cmd: &str, body: Value) -> Value {
        self.invoke(cmd, body)
            .unwrap_or_else(|error| panic!("{cmd} failed: {error}"))
    }
}

fn default_query() -> Value {
    json!({
        "search": null,
        "health": [],
        "behind_only": false,
        "provider_ids": [],
        "with_suggestions": false,
        "include_hidden": false,
        "sort": "name",
        "direction": "asc",
    })
}

#[test]
fn list_with_default_query_is_empty_and_carries_copy() {
    let ipc = ipc();

    let list = ipc.ok("list_applications", json!({ "query": default_query() }));

    assert_eq!(list["total"], 0);
    assert!(!list["copy"]["title"].as_str().unwrap().is_empty());
}

#[test]
fn application_lifecycle_decodes_typescript_payload_keys() {
    let ipc = ipc();

    let created = ipc.ok(
        "create_application",
        json!({ "input": {
            "name": "Checkout",
            "members": [{ "pipeline_id": ipc.pipeline_ids[0], "stage": null }],
        } }),
    );
    assert_eq!(created["summary"]["name"], "Checkout");
    assert_eq!(created["summary"]["capabilities"]["delete"], true);
    let id = created["summary"]["id"].as_i64().unwrap();

    let list = ipc.ok("list_applications", json!({ "query": default_query() }));
    assert_eq!(list["total"], 1);

    let fetched = ipc.ok("get_application", json!({ "id": id }));
    assert_eq!(fetched["summary"]["name"], "Checkout");

    let changes = ipc.ok(
        "get_application_changes",
        json!({ "id": id, "cursor": null, "limit": 50 }),
    );
    assert!(changes["rows"].is_array());
    assert!(changes["columns"].is_array());
    assert_eq!(changes["has_more"], false);

    let metrics = ipc.ok(
        "get_application_metrics",
        json!({ "query": {
            "application_id": id,
            "group_key": null,
            "from": "2026-10-01T00:00:00.000Z",
            "to": "2026-11-01T00:00:00.000Z",
            "period": "daily",
        } }),
    );
    assert!(metrics["metrics"].is_array());
    assert!(metrics["funnel"].is_array());

    let memberships = ipc.ok(
        "get_pipeline_applications",
        json!({ "pipelineId": ipc.pipeline_ids[0] }),
    );
    assert_eq!(memberships[0]["id"], id);

    let renamed = ipc.ok(
        "edit_application",
        json!({ "id": id, "edit": { "type": "rename", "name": "Checkout API" } }),
    );
    assert_eq!(renamed["summary"]["name"], "Checkout API");
    let override_id = renamed["overrides"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["kind"] == "rename")
        .and_then(|entry| entry["id"].as_i64())
        .unwrap();

    let reverted = ipc.ok(
        "revert_application_override",
        json!({ "id": id, "overrideId": override_id }),
    );
    assert_eq!(reverted["summary"]["name"], "Checkout");

    ipc.ok("delete_application", json!({ "id": id }));
    assert!(ipc.invoke("get_application", json!({ "id": id })).is_err());
}

#[test]
fn suggestion_and_pipeline_commands_decode_typescript_payload_keys() {
    let ipc = ipc();

    assert!(ipc.ok("list_application_suggestions", json!({})).is_array());
    ipc.ok("accept_application_suggestions", json!({ "ids": [] }));

    let known = ipc.pipeline_ids[0].clone();
    for pipeline_id in [known.as_str(), "stub__1__missing"] {
        if let Ok(applications) = ipc.invoke(
            "get_pipeline_applications",
            json!({ "pipelineId": pipeline_id }),
        ) {
            assert!(applications.is_array());
        }
        if let Ok(locations) = ipc.invoke(
            "get_commit_locations",
            json!({ "pipelineId": pipeline_id, "runNumber": 1 }),
        ) {
            assert!(locations["entries"].is_array());
        }
        if let Ok(changes) = ipc.invoke(
            "get_deploy_changes",
            json!({ "pipelineId": pipeline_id, "runNumber": 1 }),
        ) {
            assert!(changes["commits"].is_array());
        }
    }
}

#[test]
fn snake_case_payload_keys_are_rejected_as_decoding_errors() {
    let ipc = ipc();

    let error = ipc
        .raw(
            "get_commit_locations",
            json!({ "pipeline_id": "stub__1__missing", "run_number": 1 }),
        )
        .unwrap_err();

    assert!(error.as_str().unwrap().contains("missing required key"));
}
