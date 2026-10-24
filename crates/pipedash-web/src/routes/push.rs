use axum::extract::{
    Path,
    State,
};
use axum::http::StatusCode;
use axum::routing::{
    delete,
    get,
    post,
};
use axum::{
    Json,
    Router,
};
use chrono::{
    DateTime,
    Utc,
};
use pipedash_core::domain::PushSubscription;
use serde::{
    Deserialize,
    Serialize,
};

use crate::error::{
    ApiError,
    ApiResult,
    AppError,
};
use crate::push::{
    Alert,
    PushServices,
    RelayOutcome,
};
use crate::state::AppState;

const PLATFORM_IOS: &str = "ios";
const MAX_DEVICE_NAME_CHARS: usize = 100;
const MAX_SUBSCRIPTION_ID_LEN: usize = 128;

#[derive(Debug, Serialize)]
pub struct SubscriptionResponse {
    pub id: String,
    pub device_name: String,
    pub platform: String,
    pub created_at: DateTime<Utc>,
    pub last_success_at: Option<DateTime<Utc>>,
}

impl From<PushSubscription> for SubscriptionResponse {
    fn from(subscription: PushSubscription) -> Self {
        Self {
            id: subscription.id,
            device_name: subscription.device_name,
            platform: subscription.platform,
            created_at: subscription.created_at,
            last_success_at: subscription.last_success_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ListSubscriptionsResponse {
    pub relay_configured: bool,
    pub subscriptions: Vec<SubscriptionResponse>,
}

#[derive(Deserialize)]
pub struct CreateSubscriptionRequest {
    pub subscription_id: String,
    pub device_name: String,
    pub platform: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/subscriptions",
            get(list_subscriptions).post(create_subscription),
        )
        .route("/subscriptions/{id}", delete(delete_subscription))
        .route("/subscriptions/{id}/test", post(test_subscription))
}

fn relay_not_configured() -> AppError {
    AppError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        ApiError::new(
            "PUSH_RELAY_NOT_CONFIGURED",
            "push relay is not configured on this server",
        ),
    )
}

fn is_relay_subscription_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_SUBSCRIPTION_ID_LEN
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

async fn list_subscriptions(
    State(state): State<AppState>,
) -> ApiResult<Json<ListSubscriptionsResponse>> {
    let services = PushServices::from_state(&state).await?;
    let subscriptions = services.subscriptions.list().await?;

    Ok(Json(ListSubscriptionsResponse {
        relay_configured: services.relay_url().await.is_some(),
        subscriptions: subscriptions.into_iter().map(Into::into).collect(),
    }))
}

async fn create_subscription(
    State(state): State<AppState>, Json(req): Json<CreateSubscriptionRequest>,
) -> ApiResult<(StatusCode, Json<SubscriptionResponse>)> {
    let services = PushServices::from_state(&state).await?;
    if services.relay_url().await.is_none() {
        return Err(relay_not_configured());
    }

    let device_name = req.device_name.trim();
    if req.platform != PLATFORM_IOS {
        return Err(AppError::bad_request("platform must be \"ios\""));
    }
    if !is_relay_subscription_id(&req.subscription_id) {
        return Err(AppError::bad_request(
            "subscription_id must be the id issued by the push relay",
        ));
    }
    if device_name.is_empty() || device_name.chars().count() > MAX_DEVICE_NAME_CHARS {
        return Err(AppError::bad_request(
            "device_name must be 1 to 100 characters",
        ));
    }

    let subscription = services
        .subscriptions
        .register(&req.subscription_id, device_name, &req.platform)
        .await?;

    Ok((StatusCode::CREATED, Json(subscription.into())))
}

async fn delete_subscription(
    State(state): State<AppState>, Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let services = PushServices::from_state(&state).await?;
    services.subscriptions.delete(&id).await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn test_subscription(
    State(state): State<AppState>, Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let services = PushServices::from_state(&state).await?;
    let relay_url = services
        .relay_url()
        .await
        .ok_or_else(relay_not_configured)?;
    let subscription = services
        .subscriptions
        .get(&id)
        .await?
        .ok_or_else(|| AppError::not_found("push subscription not found"))?;
    let alert = Alert {
        title: "Pipedash test notification".to_string(),
        body: "Push notifications from this server are working.".to_string(),
        thread_id: None,
    };

    match services.deliver(&relay_url, &subscription, &alert).await {
        RelayOutcome::Delivered => Ok(StatusCode::ACCEPTED),
        RelayOutcome::Gone => Err(AppError::not_found(
            "push subscription is no longer registered with the relay",
        )),
        RelayOutcome::RateLimited => Err(AppError::new(
            StatusCode::TOO_MANY_REQUESTS,
            ApiError::new("RATE_LIMITED", "push relay rate limit exceeded"),
        )),
        RelayOutcome::Failed => Err(AppError::new(
            StatusCode::BAD_GATEWAY,
            ApiError::new(
                "PUSH_RELAY_ERROR",
                "push relay did not accept the notification",
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::{
        Arc,
        Mutex,
    };
    use std::time::Duration;

    use axum::body::Body;
    use axum::http::{
        Method,
        Request,
        Uri,
    };
    use pipedash_core::event::{
        CoreEvent,
        EventBus,
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
    use crate::push::spawn_failure_notifier;
    use crate::routes::CONFIG_ENV;
    use crate::ws::WebSocketEventBus;

    #[derive(Clone)]
    struct RelayState {
        requests: Arc<Mutex<Vec<Value>>>,
        status: Arc<Mutex<u16>>,
    }

    struct StubRelay {
        url: String,
        state: RelayState,
    }

    async fn accept_notification(
        State(relay): State<RelayState>, Json(body): Json<Value>,
    ) -> StatusCode {
        relay.requests.lock().unwrap().push(body);
        let status = *relay.status.lock().unwrap();

        StatusCode::from_u16(status).unwrap()
    }

    impl StubRelay {
        async fn start() -> Self {
            let state = RelayState {
                requests: Arc::default(),
                status: Arc::new(Mutex::new(202)),
            };
            let app = Router::new()
                .route("/v1/notifications", post(accept_notification))
                .with_state(state.clone());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

            Self { url, state }
        }

        fn reply_with(&self, status: u16) {
            *self.state.status.lock().unwrap() = status;
        }

        fn requests(&self) -> Vec<Value> {
            self.state.requests.lock().unwrap().clone()
        }
    }

    #[derive(Clone)]
    struct StubJenkins {
        build: Arc<Mutex<(i64, &'static str)>>,
    }

    fn build_json(number: i64, result: &str) -> Value {
        json!({
            "number": number,
            "result": result,
            "building": false,
            "timestamp": 1_700_000_000_000_i64 + number * 1000,
            "duration": 1000,
        })
    }

    async fn jenkins_job(State(stub): State<StubJenkins>, uri: Uri) -> Json<Value> {
        let (number, result) = *stub.build.lock().unwrap();

        if uri.query().is_some_and(|query| query.contains("builds")) {
            Json(json!({ "builds": [build_json(number, result)] }))
        } else {
            Json(json!({ "name": "build", "lastBuild": { "number": number } }))
        }
    }

    async fn start_jenkins() -> String {
        let stub = StubJenkins {
            build: Arc::new(Mutex::new((2, "FAILURE"))),
        };
        let app = Router::new()
            .route("/job/build/api/json", get(jenkins_job))
            .with_state(stub);
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

        let relay = StubRelay::start().await;
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
                name: "jenkins".into(),
                provider_type: "jenkins".into(),
                token: String::new(),
                config: [
                    ("server_url".to_string(), start_jenkins().await),
                    ("username".to_string(), "admin".to_string()),
                    ("selected_items".to_string(), "build".to_string()),
                ]
                .into(),
                refresh_interval: 30,
                version: None,
            })
            .await
            .unwrap();
        storage_manager
            .token_store()
            .await
            .store_token(provider_id, "jenkins-token")
            .await
            .unwrap();

        let ws_event_bus = Arc::new(WebSocketEventBus::new());
        let core = CoreContext::with_storage_manager(&storage_manager, ws_event_bus.clone())
            .await
            .unwrap();
        let state = AppState::initialized(ws_event_bus, core, storage_manager);
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

    async fn call(
        app: &Router, method: Method, uri: &str, body: Option<Value>,
    ) -> (StatusCode, Value) {
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

    async fn register(h: &Harness, relay_id: &str, device_name: &str) -> Value {
        let (status, body) = call(
            &h.app,
            Method::POST,
            "/push/subscriptions",
            Some(json!({
                "subscription_id": relay_id,
                "device_name": device_name,
                "platform": "ios",
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");

        body
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

    fn jenkins_pipeline(provider_id: i64) -> pipedash_core::Pipeline {
        serde_json::from_value(json!({
            "id": format!("jenkins__{provider_id}__build"),
            "provider_id": provider_id,
            "provider_type": "jenkins",
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
    async fn a_device_registers_idempotently_and_the_relay_id_is_never_returned() {
        let h = harness("register", true).await;

        let first = register(&h, "relay-secret_1", "  Henrique's iPhone ").await;
        let again = register(&h, "relay-secret_1", "Renamed").await;
        let list = listed(&h).await;

        assert_eq!(first["device_name"], "Henrique's iPhone");
        assert_eq!(first["platform"], "ios");
        assert_eq!(first["last_success_at"], Value::Null);
        assert_eq!(first["id"], again["id"]);
        assert_eq!(list["relay_configured"], true);
        assert_eq!(list["subscriptions"].as_array().unwrap().len(), 1);
        assert_eq!(list["subscriptions"][0]["id"], first["id"]);
        for body in [&first.to_string(), &list.to_string()] {
            assert!(!body.contains("relay-secret_1"), "{body}");
        }
    }

    #[tokio::test]
    async fn deleting_a_device_removes_it_and_can_be_repeated() {
        let h = harness("delete", true).await;
        let sub = register(&h, "relay-secret_1", "iPhone").await;
        let uri = format!("/push/subscriptions/{}", sub["id"].as_str().unwrap());

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
    async fn invalid_registrations_are_rejected() {
        let h = harness("invalid", true).await;
        let long_name = "n".repeat(MAX_DEVICE_NAME_CHARS + 1);

        for (relay_id, name, platform) in [
            ("relay-secret_1", "iPhone", "android"),
            ("bad id!", "iPhone", "ios"),
            ("", "iPhone", "ios"),
            ("relay-secret_1", "   ", "ios"),
            ("relay-secret_1", long_name.as_str(), "ios"),
        ] {
            let (status, _) = call(
                &h.app,
                Method::POST,
                "/push/subscriptions",
                Some(json!({ "subscription_id": relay_id, "device_name": name, "platform": platform })),
            )
            .await;

            assert_eq!(
                status,
                StatusCode::BAD_REQUEST,
                "{relay_id} {name} {platform}"
            );
        }
        assert!(listed(&h).await["subscriptions"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn without_a_relay_url_registering_and_testing_are_refused() {
        let h = harness("unconfigured", false).await;

        let (status, body) = call(
            &h.app,
            Method::POST,
            "/push/subscriptions",
            Some(json!({ "subscription_id": "relay-secret_1", "device_name": "iPhone", "platform": "ios" })),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["code"], "PUSH_RELAY_NOT_CONFIGURED");
        assert_eq!(
            body["message"],
            "push relay is not configured on this server"
        );

        let (status, _) = call(&h.app, Method::POST, "/push/subscriptions/any/test", None).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(listed(&h).await["relay_configured"], false);
    }

    #[tokio::test]
    async fn the_test_route_pushes_through_the_relay_and_records_the_success() {
        let h = harness("test-route", true).await;
        let sub = register(&h, "relay-secret_1", "iPhone").await;
        let uri = format!("/push/subscriptions/{}/test", sub["id"].as_str().unwrap());

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
        let sub = register(&h, "relay-secret_1", "iPhone").await;
        let uri = format!("/push/subscriptions/{}/test", sub["id"].as_str().unwrap());

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
        register(&h, "relay-secret_1", "iPhone").await;
        register(&h, "relay-secret_2", "iPad").await;
        let pipeline = jenkins_pipeline(h.provider_id);
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
                    "body": "jenkins · #2",
                    "thread_id": pipeline.id,
                }),
                json!({
                    "subscription_id": "relay-secret_2",
                    "title": "build failed",
                    "body": "jenkins · #2",
                    "thread_id": pipeline.id,
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
        register(&h, "relay-secret_1", "iPhone").await;
        h.relay.reply_with(404);
        spawn_failure_notifier(h.state.clone());

        h.state
            .ws_event_bus
            .emit(CoreEvent::PipelinesFailed {
                pipelines: vec![jenkins_pipeline(h.provider_id)],
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
                pipelines: vec![jenkins_pipeline(h.provider_id)],
            })
            .await;
        tokio::time::sleep(Duration::from_millis(300)).await;

        assert!(h.relay.requests().is_empty());
    }
}
