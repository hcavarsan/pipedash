use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{
    Method,
    Request,
};
use pipedash_core::event::{
    CoreEvent,
    EventBus,
};
use pipedash_core::infrastructure::database::PushRepository;
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
use crate::push::spawn_failure_notifier;
use crate::routes::CONFIG_ENV;
use crate::test_support::{
    stub_server,
    StubRelay,
};
use crate::ws::event_bus::WebSocketEventBus;

async fn list_runs() -> Json<Value> {
    Json(json!({
        "runs": [{
            "number": 2,
            "status": "failed",
            "timestamp": 1_700_000_002_000_i64,
            "duration": 1000,
        }]
    }))
}

async fn start_stub_server() -> String {
    let app = Router::new().route("/runs", get(list_runs));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    url
}

struct Harness {
    state: AppState,
    app: Router,
    relay: StubRelay,
    provider_id: i64,
    data_dir: PathBuf,
    _env: tokio::sync::MutexGuard<'static, ()>,
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.data_dir);
    }
}

async fn harness(name: &str, relay_configured: bool) -> Harness {
    let env = CONFIG_ENV.lock().await;
    let data_dir =
        std::env::temp_dir().join(format!("pipedash-push-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).unwrap();

    let relay = stub_server().await;
    let config_file = if relay_configured {
        format!("[push]\nrelay_url = \"{}/\"\n", relay.url)
    } else {
        String::new()
    };
    std::fs::write(data_dir.join("config.toml"), config_file).unwrap();
    std::env::set_var("PIPEDASH_CONFIG_PATH", data_dir.join("config.toml"));
    std::env::remove_var("PIPEDASH_PUSH_RELAY_URL");

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
            config: [("server_url".to_string(), start_stub_server().await)].into(),
            refresh_interval: 30,
            version: None,
        })
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
    let state = AppState::initialized(
        ws_event_bus,
        core,
        storage_manager,
        crate::test_support::registry(),
    );
    let app = Router::new()
        .nest("/push", router())
        .with_state(state.clone());

    Harness {
        state,
        app,
        relay,
        provider_id,
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

async fn seed(h: &Harness, relay_id: &str, device_name: &str) -> String {
    let subscription = PushSubscription {
        id: PushSubscription::local_id(relay_id),
        relay_subscription_id: relay_id.into(),
        device_name: device_name.into(),
        platform: "ios".into(),
        created_at: chrono::Utc::now(),
        last_success_at: None,
    };
    PushRepository::new(h.state.storage_manager().await.unwrap().database_pool())
        .import(std::slice::from_ref(&subscription))
        .await
        .unwrap();

    subscription.id
}

async fn listed(h: &Harness) -> Value {
    let (status, body) = call(&h.app, Method::GET, "/push/subscriptions", None).await;
    assert_eq!(status, StatusCode::OK);

    body
}

async fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let wait = async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };

    if tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .is_err()
    {
        panic!("timed out waiting for {what}");
    }
}

async fn wait_for_list(h: &Harness, what: &str, ready: impl Fn(&Value) -> bool) {
    let wait = async {
        while !ready(&listed(h).await) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };

    if tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .is_err()
    {
        panic!("timed out waiting for {what}");
    }
}

fn stub_pipeline(provider_id: i64) -> pipedash_core::Pipeline {
    serde_json::from_value(json!({
        "id": format!("stub__{provider_id}__build"),
        "provider_id": provider_id,
        "provider_type": "stub",
        "name": "build",
        "status": "failed",
        "last_run": null,
        "last_updated": chrono::Utc::now(),
        "repository": "",
        "branch": null,
        "workflow_file": null,
    }))
    .unwrap()
}

#[tokio::test]
async fn a_factory_reset_forgets_every_paired_device() {
    let h = harness("reset", true).await;
    seed(&h, "relay-secret_1", "iPhone").await;
    seed(&h, "relay-secret_2", "iPad").await;
    let app = h
        .app
        .clone()
        .merge(crate::routes::system::router().with_state(h.state.clone()));

    let (status, _) = call(&app, Method::POST, "/factory-reset", None).await;

    assert_eq!(status, StatusCode::OK);
    assert!(listed(&h).await["subscriptions"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn deleting_a_device_removes_it_and_can_be_repeated() {
    let h = harness("delete", true).await;
    let id = seed(&h, "relay-secret_1", "iPhone").await;
    let uri = format!("/push/subscriptions/{id}");

    assert_eq!(
        call(&h.app, Method::DELETE, &uri, None).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        call(&h.app, Method::DELETE, &uri, None).await.0,
        StatusCode::NO_CONTENT
    );
    assert!(listed(&h).await["subscriptions"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn without_a_relay_url_testing_is_refused() {
    let h = harness("unconfigured", false).await;

    let (status, body) = call(&h.app, Method::POST, "/push/subscriptions/any/test", None).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "PUSH_RELAY_NOT_CONFIGURED");
    assert_eq!(
        body["message"],
        "push relay is not configured on this server"
    );
    assert_eq!(listed(&h).await["relay_configured"], false);
}

#[tokio::test]
async fn the_test_route_pushes_through_the_relay_and_records_the_success() {
    let h = harness("test-route", true).await;
    let id = seed(&h, "relay-secret_1", "iPhone").await;
    let uri = format!("/push/subscriptions/{id}/test");

    let (status, _) = call(&h.app, Method::POST, &uri, None).await;

    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(
        h.relay.requests(),
        vec![json!({
            "subscription_id": "relay-secret_1",
            "title": "Pipedash test notification",
            "body": "Push notifications from this server are working.",
        })]
    );
    assert!(listed(&h).await["subscriptions"][0]["last_success_at"].is_string());
}

#[tokio::test]
async fn relay_answers_to_a_test_push_map_to_statuses_and_gone_forgets_the_device() {
    let h = harness("test-statuses", true).await;
    let id = seed(&h, "relay-secret_1", "iPhone").await;
    let uri = format!("/push/subscriptions/{id}/test");

    for (relay_status, expected) in [
        (429, StatusCode::TOO_MANY_REQUESTS),
        (500, StatusCode::BAD_GATEWAY),
        (403, StatusCode::BAD_GATEWAY),
    ] {
        h.relay.reply_with(relay_status);
        assert_eq!(call(&h.app, Method::POST, &uri, None).await.0, expected);
        assert_eq!(
            listed(&h).await["subscriptions"].as_array().unwrap().len(),
            1
        );
    }

    h.relay.reply_with(410);
    assert_eq!(
        call(&h.app, Method::POST, &uri, None).await.0,
        StatusCode::NOT_FOUND
    );
    assert!(listed(&h).await["subscriptions"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        call(&h.app, Method::POST, &uri, None).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn push_routes_need_the_api_token() {
    let h = harness("auth", true).await;
    h.state.set_api_token("secret-token");
    let app = crate::app(h.state.clone(), None);
    let get = |token: Option<&str>| {
        let mut request = Request::get("/api/v1/push/subscriptions");
        if let Some(token) = token {
            request = request.header("authorization", format!("Bearer {token}"));
        }
        app.clone().oneshot(request.body(Body::empty()).unwrap())
    };

    assert_eq!(get(None).await.unwrap().status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        get(Some("wrong")).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get(Some("secret-token")).await.unwrap().status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_failed_pipeline_reaches_every_device_once_with_provider_and_failed_run() {
    let h = harness("notify", true).await;
    seed(&h, "relay-secret_1", "iPhone").await;
    seed(&h, "relay-secret_2", "iPad").await;
    let pipeline = stub_pipeline(h.provider_id);
    h.state
        .provider_service()
        .await
        .ok()
        .unwrap()
        .repository()
        .update_pipelines_cache(h.provider_id, std::slice::from_ref(&pipeline))
        .await
        .unwrap();
    spawn_failure_notifier(h.state.clone());

    h.state
        .ws_event_bus
        .emit(CoreEvent::PipelinesFailed {
            pipelines: vec![pipeline.clone()],
        })
        .await;
    wait_for("both pushes", || h.relay.requests().len() == 2).await;

    let mut requests = h.relay.requests();
    requests.sort_by_key(|request| request["subscription_id"].to_string());
    assert_eq!(
        requests,
        vec![
            json!({
                "subscription_id": "relay-secret_1",
                "title": "build failed",
                "body": "stub · #2",
                "thread_id": pipeline.id,
                "pipeline_id": pipeline.id,
                "provider_id": pipeline.provider_id,
                "run_number": 2,
            }),
            json!({
                "subscription_id": "relay-secret_2",
                "title": "build failed",
                "body": "stub · #2",
                "thread_id": pipeline.id,
                "pipeline_id": pipeline.id,
                "provider_id": pipeline.provider_id,
                "run_number": 2,
            }),
        ]
    );
    wait_for_list(&h, "both pushes to be recorded", |list| {
        list["subscriptions"]
            .as_array()
            .unwrap()
            .iter()
            .all(|sub| sub["last_success_at"].is_string())
    })
    .await;
}

#[tokio::test]
async fn a_device_the_relay_no_longer_knows_is_forgotten_after_a_failure_push() {
    let h = harness("gone", true).await;
    seed(&h, "relay-secret_1", "iPhone").await;
    h.relay.reply_with(404);
    spawn_failure_notifier(h.state.clone());

    h.state
        .ws_event_bus
        .emit(CoreEvent::PipelinesFailed {
            pipelines: vec![stub_pipeline(h.provider_id)],
        })
        .await;

    wait_for("the push", || h.relay.requests().len() == 1).await;
    wait_for_list(&h, "the device to be forgotten", |list| {
        list["subscriptions"].as_array().unwrap().is_empty()
    })
    .await;
}

#[tokio::test]
async fn failures_with_no_registered_device_send_nothing() {
    let h = harness("nobody", true).await;
    spawn_failure_notifier(h.state.clone());

    h.state
        .ws_event_bus
        .emit(CoreEvent::PipelinesFailed {
            pipelines: vec![stub_pipeline(h.provider_id)],
        })
        .await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert!(h.relay.requests().is_empty());
}

#[tokio::test]
async fn the_operator_turns_the_official_relay_on_and_off() {
    let h = harness("relay-toggle", false).await;
    let config_file = h.data_dir.join("config.toml");

    let (status, body) = call(&h.app, Method::GET, "/push/relay", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        body,
        json!({
            "enabled": false,
            "relay_url": null,
            "from_env": false,
            "official_url": "https://relay.pipedash.app",
        })
    );

    let (status, body) = call(
        &h.app,
        Method::PUT,
        "/push/relay",
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["enabled"], true);
    assert_eq!(body["relay_url"], "https://relay.pipedash.app");
    assert!(std::fs::read_to_string(&config_file)
        .unwrap()
        .contains("[push]\nrelay_url = \"https://relay.pipedash.app\""));
    assert_eq!(
        h.state.push_relay_url().await.as_deref(),
        Some("https://relay.pipedash.app")
    );
    assert_eq!(listed(&h).await["relay_configured"], true);

    let (status, body) = call(
        &h.app,
        Method::PUT,
        "/push/relay",
        Some(json!({ "enabled": false })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["enabled"], false);
    assert_eq!(body["relay_url"], Value::Null);
    assert!(!std::fs::read_to_string(&config_file)
        .unwrap()
        .contains("relay_url"));
}

#[tokio::test]
async fn a_relay_set_by_the_environment_cannot_be_changed_from_the_api() {
    let h = harness("relay-from-env", false).await;
    std::env::set_var("PIPEDASH_PUSH_RELAY_URL", "https://env.example.com/");

    let (status, body) = call(&h.app, Method::GET, "/push/relay", None).await;
    let (put_status, put_body) = call(
        &h.app,
        Method::PUT,
        "/push/relay",
        Some(json!({ "enabled": true })),
    )
    .await;
    std::env::remove_var("PIPEDASH_PUSH_RELAY_URL");

    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["from_env"], true);
    assert_eq!(body["enabled"], true);
    assert_eq!(body["relay_url"], "https://env.example.com");
    assert_eq!(put_status, StatusCode::CONFLICT);
    assert_eq!(put_body["code"], "PUSH_RELAY_FROM_ENV");
    assert_eq!(
        put_body["message"],
        "PIPEDASH_PUSH_RELAY_URL sets the relay. Change it in the server's environment."
    );
    assert!(!std::fs::read_to_string(h.data_dir.join("config.toml"))
        .unwrap()
        .contains("relay_url"));
}
