use std::collections::HashSet;
use std::net::{
    IpAddr,
    SocketAddr,
};
use std::sync::Arc;
use std::time::{
    Duration,
    Instant,
};

use axum::extract::{
    ConnectInfo,
    DefaultBodyLimit,
    Path,
    State,
};
use axum::http::{
    HeaderMap,
    StatusCode,
};
use axum::response::{
    IntoResponse,
    Response,
};
use axum::routing::{
    get,
    post,
    put,
};
use axum::{
    Json,
    Router,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ring::rand::{
    SecureRandom,
    SystemRandom,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::apns::{
    unix_now,
    Apns,
    Notification,
    Outcome,
};
use crate::rate_limit::RateLimiter;
use crate::store::{
    subscription_key,
    Environment,
    Store,
    Subscription,
};

const MAX_REQUEST_BYTES: usize = 16 * 1024;
const MIN_TOKEN_HEX_LEN: usize = 64;
const MAX_TOKEN_HEX_LEN: usize = 200;
const MAX_TITLE_BYTES: usize = 512;
const MAX_BODY_BYTES: usize = 2048;
const MAX_THREAD_ID_BYTES: usize = 512;
const RATE_LIMIT: u32 = 30;
const RATE_WINDOW: Duration = Duration::from_secs(60);
const REGISTRATION_LIMIT: u32 = 20;
const REGISTRATION_WINDOW: Duration = Duration::from_secs(60 * 60);
const MAX_SUBSCRIPTIONS_PER_TOKEN: u32 = 10;

#[derive(Clone)]
pub struct AppState {
    store: Store,
    apns: Arc<Apns>,
    limiter: Arc<RateLimiter>,
    registrations: Arc<RateLimiter>,
    allowed_bundle_ids: Arc<HashSet<String>>,
    trusted_proxy: bool,
}

impl AppState {
    pub fn new(
        store: Store, apns: Arc<Apns>, allowed_bundle_ids: HashSet<String>, trusted_proxy: bool,
    ) -> Self {
        Self {
            store,
            apns,
            limiter: Arc::new(RateLimiter::new(RATE_LIMIT, RATE_WINDOW)),
            registrations: Arc::new(RateLimiter::new(REGISTRATION_LIMIT, REGISTRATION_WINDOW)),
            allowed_bundle_ids: Arc::new(allowed_bundle_ids),
            trusted_proxy,
        }
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/subscriptions", post(create))
        .route(
            "/v1/subscriptions/{subscription_id}",
            put(rotate).delete(remove),
        )
        .route("/v1/notifications", post(notify))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .with_state(state)
}

enum ApiError {
    BadRequest(&'static str),
    Forbidden,
    NotFound,
    Gone,
    TooManyRequests,
    BadGateway,
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::Forbidden => (StatusCode::FORBIDDEN, "bundle id not allowed"),
            Self::NotFound => (StatusCode::NOT_FOUND, "unknown subscription"),
            Self::Gone => (StatusCode::GONE, "device token is no longer registered"),
            Self::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded"),
            Self::BadGateway => (
                StatusCode::BAD_GATEWAY,
                "APNs did not accept the notification",
            ),
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal error"),
        };

        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(%error, "Database error");
        Self::Internal
    }
}

#[derive(Deserialize)]
struct CreateRequest {
    device_token: String,
    environment: Environment,
    bundle_id: String,
}

#[derive(Serialize)]
struct Created {
    subscription_id: String,
}

#[derive(Deserialize)]
struct RotateRequest {
    device_token: String,
}

#[derive(Deserialize)]
struct NotifyRequest {
    subscription_id: String,
    title: String,
    body: String,
    thread_id: Option<String>,
}

// The token goes into the APNs URL path, so anything but hex is refused.
fn normalize_token(token: &str) -> Result<String, ApiError> {
    let valid = (MIN_TOKEN_HEX_LEN..=MAX_TOKEN_HEX_LEN).contains(&token.len())
        && token.len() % 2 == 0
        && token.bytes().all(|b| b.is_ascii_hexdigit());

    if valid {
        Ok(token.to_ascii_lowercase())
    } else {
        Err(ApiError::BadRequest("invalid device token"))
    }
}

fn new_subscription_id() -> Result<String, ApiError> {
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| ApiError::Internal)?;

    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

// Behind a trusted reverse proxy the client is the last X-Forwarded-For entry,
// the one the proxy appended. IPv6 clients are grouped by /64 because a single
// host owns a whole prefix.
fn client_key(peer: IpAddr, headers: &HeaderMap, trusted_proxy: bool) -> String {
    let forwarded = trusted_proxy
        .then(|| {
            headers
                .get_all("x-forwarded-for")
                .iter()
                .next_back()
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.rsplit(',').next())
                .and_then(|entry| entry.trim().parse::<IpAddr>().ok())
        })
        .flatten();

    match forwarded.unwrap_or(peer) {
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => {
            let segments = ip.segments();
            format!(
                "{:x}:{:x}:{:x}:{:x}::/64",
                segments[0], segments[1], segments[2], segments[3]
            )
        }
    }
}

async fn healthz() -> &'static str {
    "ok"
}

async fn create(
    State(state): State<AppState>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap,
    Json(request): Json<CreateRequest>,
) -> Result<(StatusCode, Json<Created>), ApiError> {
    let client = client_key(peer.ip(), &headers, state.trusted_proxy);
    if !state.registrations.allow(&client, Instant::now()) {
        return Err(ApiError::TooManyRequests);
    }

    let device_token = normalize_token(&request.device_token)?;
    if !state.allowed_bundle_ids.contains(&request.bundle_id) {
        return Err(ApiError::Forbidden);
    }

    let subscription_id = new_subscription_id()?;
    let subscription = Subscription {
        device_token,
        environment: request.environment,
        bundle_id: request.bundle_id,
    };
    state
        .store
        .insert(
            &subscription_key(&subscription_id),
            &subscription,
            unix_now(),
            MAX_SUBSCRIPTIONS_PER_TOKEN,
        )
        .await?;

    Ok((StatusCode::CREATED, Json(Created { subscription_id })))
}

async fn rotate(
    State(state): State<AppState>, Path(subscription_id): Path<String>,
    Json(request): Json<RotateRequest>,
) -> Result<StatusCode, ApiError> {
    let device_token = normalize_token(&request.device_token)?;

    if state
        .store
        .update_token(
            &subscription_key(&subscription_id),
            &device_token,
            unix_now(),
        )
        .await?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

async fn remove(
    State(state): State<AppState>, Path(subscription_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state
        .store
        .delete(&subscription_key(&subscription_id))
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn notify(
    State(state): State<AppState>, Json(request): Json<NotifyRequest>,
) -> Result<StatusCode, ApiError> {
    if request.title.is_empty()
        || request.title.len() > MAX_TITLE_BYTES
        || request.body.len() > MAX_BODY_BYTES
        || request
            .thread_id
            .as_ref()
            .is_some_and(|thread_id| thread_id.len() > MAX_THREAD_ID_BYTES)
    {
        return Err(ApiError::BadRequest("invalid notification"));
    }

    let key = subscription_key(&request.subscription_id);
    let Some(subscription) = state.store.get(&key).await? else {
        return Err(ApiError::NotFound);
    };
    if !state.limiter.allow(&key, Instant::now()) {
        return Err(ApiError::TooManyRequests);
    }

    let notification = Notification {
        title: &request.title,
        body: &request.body,
        thread_id: request.thread_id.as_deref(),
    };
    match state.apns.send(&subscription, &notification).await {
        Outcome::Delivered => {
            if let Err(error) = state.store.touch(&key, unix_now()).await {
                tracing::warn!(%error, "Couldn't record a successful push");
            }
            Ok(StatusCode::ACCEPTED)
        }
        Outcome::Unregistered => {
            state.store.delete(&key).await?;
            Err(ApiError::Gone)
        }
        Outcome::Failed => Err(ApiError::BadGateway),
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::header::CONTENT_TYPE;
    use axum::http::{
        Method,
        Request,
    };
    use serde_json::{
        json,
        Value,
    };
    use tower::ServiceExt;

    use super::*;
    use crate::apns::Signer;
    use crate::test_support::{
        key_pem,
        StubApns,
    };

    const BUNDLE: &str = "com.example.app";

    struct Harness {
        app: Router,
        store: Store,
        production: StubApns,
        sandbox: StubApns,
        _dir: tempfile::TempDir,
    }

    async fn harness() -> Harness {
        harness_behind(false).await
    }

    async fn harness_behind(trusted_proxy: bool) -> Harness {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("relay.db")).await.unwrap();
        let production = StubApns::start().await;
        let sandbox = StubApns::start().await;
        let (pem, _) = key_pem();
        let apns = Apns::new(
            Signer::new(&pem, "K", "T").unwrap(),
            &production.url,
            &sandbox.url,
        )
        .unwrap();
        let state = AppState::new(
            store.clone(),
            Arc::new(apns),
            HashSet::from([BUNDLE.to_string()]),
            trusted_proxy,
        );

        Harness {
            app: router(state),
            store,
            production,
            sandbox,
            _dir: dir,
        }
    }

    async fn call(
        app: &Router, method: Method, uri: &str, body: Option<Value>,
    ) -> (StatusCode, Value) {
        call_as(app, "203.0.113.1:4000", &[], method, uri, body).await
    }

    async fn call_as(
        app: &Router, peer: &str, headers: &[(&str, &str)], method: Method, uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder().method(method).uri(uri);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let body = match body {
            Some(value) => {
                builder = builder.header(CONTENT_TYPE, "application/json");
                Body::from(value.to_string())
            }
            None => Body::empty(),
        };
        let mut request = builder.body(body).unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(peer.parse::<SocketAddr>().unwrap()));
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();

        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    fn token(byte: &str) -> String {
        byte.repeat(32)
    }

    async fn subscribe(h: &Harness, environment: &str, device_token: &str) -> String {
        let (status, body) = call(
            &h.app,
            Method::POST,
            "/v1/subscriptions",
            Some(json!({
                "device_token": device_token,
                "environment": environment,
                "bundle_id": BUNDLE,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);

        body["subscription_id"].as_str().unwrap().to_string()
    }

    async fn push(h: &Harness, subscription_id: &str) -> (StatusCode, Value) {
        call(
            &h.app,
            Method::POST,
            "/v1/notifications",
            Some(json!({
                "subscription_id": subscription_id,
                "title": "build failed",
                "body": "GitHub · main · #7",
                "thread_id": "github__1__build",
            })),
        )
        .await
    }

    #[tokio::test]
    async fn healthz_answers_without_touching_anything() {
        let h = harness().await;
        let response = h
            .app
            .clone()
            .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn a_new_subscription_gets_a_random_secret_id_and_only_its_hash_is_stored() {
        let h = harness().await;
        let id = subscribe(&h, "production", &"AB".repeat(32)).await;
        let other = subscribe(&h, "production", &token("ab")).await;

        assert_eq!(id.len(), 43);
        assert!(id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
        assert_ne!(id, other);
        assert_eq!(
            h.store.get(&subscription_key(&id)).await.unwrap(),
            Some(Subscription {
                device_token: token("ab"),
                environment: Environment::Production,
                bundle_id: BUNDLE.to_string(),
            })
        );
        assert_eq!(h.store.get(&id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn bad_registrations_are_refused() {
        let h = harness().await;
        let register = |token: String, environment: &str, bundle_id: &str| {
            call(
                &h.app,
                Method::POST,
                "/v1/subscriptions",
                Some(json!({
                    "device_token": token,
                    "environment": environment,
                    "bundle_id": bundle_id,
                })),
            )
        };

        assert_eq!(
            register(token("ab"), "production", "com.other.app").await.0,
            StatusCode::FORBIDDEN
        );
        let bad_tokens = [
            String::new(),
            "abcd".to_string(),
            "zz".repeat(32),
            "a".repeat(65),
            "ab".repeat(101),
        ];
        for bad in bad_tokens {
            assert_eq!(
                register(bad.clone(), "production", BUNDLE).await.0,
                StatusCode::BAD_REQUEST,
                "{bad}"
            );
        }
        assert_eq!(
            register(token("ab"), "staging", BUNDLE).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[tokio::test]
    async fn a_notification_goes_to_the_apns_host_of_the_subscription_environment() {
        let h = harness().await;
        let production = subscribe(&h, "production", &token("ab")).await;
        let sandbox = subscribe(&h, "sandbox", &token("cd")).await;

        assert_eq!(push(&h, &sandbox).await.0, StatusCode::ACCEPTED);
        assert!(h.production.take_requests().is_empty());
        let requests = h.sandbox.take_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].path, format!("/3/device/{}", token("cd")));
        assert_eq!(requests[0].topic.as_deref(), Some(BUNDLE));
        assert_eq!(requests[0].push_type.as_deref(), Some("alert"));
        assert_eq!(
            requests[0].body["aps"],
            json!({
                "alert": { "title": "build failed", "body": "GitHub · main · #7" },
                "sound": "default",
                "thread-id": "github__1__build",
            })
        );

        assert_eq!(push(&h, &production).await.0, StatusCode::ACCEPTED);
        assert!(h.sandbox.take_requests().is_empty());
        assert_eq!(
            h.production.take_requests()[0].path,
            format!("/3/device/{}", token("ab"))
        );
    }

    #[tokio::test]
    async fn an_unregistered_token_is_gone_and_the_subscription_is_deleted() {
        for (status, reason) in [(410, "Unregistered"), (400, "BadDeviceToken")] {
            let h = harness().await;
            let id = subscribe(&h, "production", &token("ab")).await;
            h.production
                .reply_with(status, &json!({ "reason": reason }).to_string());

            assert_eq!(push(&h, &id).await.0, StatusCode::GONE, "{reason}");
            assert_eq!(h.store.get(&subscription_key(&id)).await.unwrap(), None);
            h.production.reply_with(200, "");
            assert_eq!(push(&h, &id).await.0, StatusCode::NOT_FOUND);
            assert_eq!(h.production.take_requests().len(), 1);
        }
    }

    #[tokio::test]
    async fn other_apns_failures_keep_the_subscription() {
        let h = harness().await;
        let id = subscribe(&h, "production", &token("ab")).await;

        for (status, reason) in [
            (403, "ExpiredProviderToken"),
            (400, "BadTopic"),
            (429, "TooManyRequests"),
            (500, "InternalServerError"),
        ] {
            h.production
                .reply_with(status, &json!({ "reason": reason }).to_string());

            assert_eq!(push(&h, &id).await.0, StatusCode::BAD_GATEWAY, "{reason}");
            assert!(h.store.get(&subscription_key(&id)).await.unwrap().is_some());
        }
    }

    #[tokio::test]
    async fn unknown_subscriptions_are_not_found_and_never_reach_apns() {
        let h = harness().await;

        assert_eq!(push(&h, "nope").await.0, StatusCode::NOT_FOUND);
        assert_eq!(push(&h, "").await.0, StatusCode::NOT_FOUND);
        assert!(h.production.take_requests().is_empty());
        assert!(h.sandbox.take_requests().is_empty());
    }

    #[tokio::test]
    async fn each_subscription_is_rate_limited_on_its_own() {
        let h = harness().await;
        let busy = subscribe(&h, "production", &token("ab")).await;
        let quiet = subscribe(&h, "production", &token("cd")).await;

        for _ in 0..RATE_LIMIT {
            assert_eq!(push(&h, &busy).await.0, StatusCode::ACCEPTED);
        }

        assert_eq!(push(&h, &busy).await.0, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(push(&h, &quiet).await.0, StatusCode::ACCEPTED);
        assert_eq!(h.production.take_requests().len(), RATE_LIMIT as usize + 1);
    }

    #[tokio::test]
    async fn rotating_the_device_token_redirects_later_pushes() {
        let h = harness().await;
        let id = subscribe(&h, "production", &token("ab")).await;
        let uri = format!("/v1/subscriptions/{id}");

        let (status, _) = call(
            &h.app,
            Method::PUT,
            &uri,
            Some(json!({ "device_token": token("ef") })),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(push(&h, &id).await.0, StatusCode::ACCEPTED);
        assert_eq!(
            h.production.take_requests()[0].path,
            format!("/3/device/{}", token("ef"))
        );

        let (status, _) = call(
            &h.app,
            Method::PUT,
            &uri,
            Some(json!({ "device_token": "xyz" })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = call(
            &h.app,
            Method::PUT,
            "/v1/subscriptions/unknown",
            Some(json!({ "device_token": token("ef") })),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn deleting_a_subscription_stops_its_pushes_and_can_be_repeated() {
        let h = harness().await;
        let id = subscribe(&h, "production", &token("ab")).await;
        let uri = format!("/v1/subscriptions/{id}");

        assert_eq!(
            call(&h.app, Method::DELETE, &uri, None).await.0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            call(&h.app, Method::DELETE, &uri, None).await.0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(push(&h, &id).await.0, StatusCode::NOT_FOUND);
        assert!(h.production.take_requests().is_empty());
    }

    #[tokio::test]
    async fn oversized_requests_and_notifications_are_refused() {
        let h = harness().await;
        let id = subscribe(&h, "production", &token("ab")).await;

        let (status, _) = call(
            &h.app,
            Method::POST,
            "/v1/notifications",
            Some(json!({ "subscription_id": id, "title": "t", "body": "x".repeat(MAX_REQUEST_BYTES) })),
        )
        .await;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);

        for (title, body, thread_id) in [
            (
                "t".repeat(MAX_TITLE_BYTES + 1),
                "b".to_string(),
                "th".to_string(),
            ),
            (
                "t".to_string(),
                "b".repeat(MAX_BODY_BYTES + 1),
                "th".to_string(),
            ),
            (
                "t".to_string(),
                "b".to_string(),
                "x".repeat(MAX_THREAD_ID_BYTES + 1),
            ),
            (String::new(), "b".to_string(), "th".to_string()),
        ] {
            let (status, _) = call(
                &h.app,
                Method::POST,
                "/v1/notifications",
                Some(json!({ "subscription_id": id, "title": title, "body": body, "thread_id": thread_id })),
            )
            .await;
            assert_eq!(status, StatusCode::BAD_REQUEST);
        }
        assert!(h.production.take_requests().is_empty());
    }

    async fn register_from(
        h: &Harness, peer: &str, headers: &[(&str, &str)], device_token: &str,
    ) -> StatusCode {
        call_as(
            &h.app,
            peer,
            headers,
            Method::POST,
            "/v1/subscriptions",
            Some(json!({
                "device_token": device_token,
                "environment": "production",
                "bundle_id": BUNDLE,
            })),
        )
        .await
        .0
    }

    #[tokio::test]
    async fn registrations_are_limited_per_client_address() {
        let h = harness().await;

        for _ in 0..REGISTRATION_LIMIT {
            let status = register_from(&h, "198.51.100.7:1000", &[], &token("ab")).await;
            assert_eq!(status, StatusCode::CREATED);
        }

        let blocked = register_from(&h, "198.51.100.7:1001", &[], &token("ab")).await;
        let other_address = register_from(&h, "198.51.100.8:1000", &[], &token("ab")).await;
        let spoofed = register_from(
            &h,
            "198.51.100.7:1002",
            &[("x-forwarded-for", "192.0.2.99")],
            &token("ab"),
        )
        .await;

        assert_eq!(blocked, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(other_address, StatusCode::CREATED);
        assert_eq!(spoofed, StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn behind_a_trusted_proxy_clients_are_told_apart_by_the_forwarded_address() {
        let h = harness_behind(true).await;
        let proxy = "10.0.0.2:5000";

        for _ in 0..REGISTRATION_LIMIT {
            let status = register_from(
                &h,
                proxy,
                &[("x-forwarded-for", "1.1.1.1, 203.0.113.9")],
                &token("ab"),
            )
            .await;
            assert_eq!(status, StatusCode::CREATED);
        }

        let same_client = register_from(
            &h,
            proxy,
            &[("x-forwarded-for", "9.9.9.9, 203.0.113.9")],
            &token("ab"),
        )
        .await;
        let other_client = register_from(
            &h,
            proxy,
            &[("x-forwarded-for", "203.0.113.10")],
            &token("ab"),
        )
        .await;

        assert_eq!(same_client, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(other_client, StatusCode::CREATED);
    }

    #[test]
    fn client_keys_group_ipv6_by_prefix_and_ignore_unparsable_forwarding() {
        let mut headers = HeaderMap::new();
        let v6: IpAddr = "2001:db8:1:2:aaaa:bbbb:cccc:dddd".parse().unwrap();
        let v6_neighbour: IpAddr = "2001:db8:1:2:1::1".parse().unwrap();

        assert_eq!(
            client_key(v6, &headers, false),
            client_key(v6_neighbour, &headers, false)
        );
        assert_eq!(client_key(v6, &headers, false), "2001:db8:1:2::/64");

        headers.insert("x-forwarded-for", "not-an-ip".parse().unwrap());
        assert_eq!(
            client_key("192.0.2.1".parse().unwrap(), &headers, true),
            "192.0.2.1"
        );
    }

    #[tokio::test]
    async fn the_oldest_subscriptions_of_a_device_token_are_dropped_past_the_cap() {
        let h = harness().await;
        let mut ids = Vec::new();
        for _ in 0..=MAX_SUBSCRIPTIONS_PER_TOKEN {
            ids.push(subscribe(&h, "production", &token("ab")).await);
        }

        assert_eq!(push(&h, &ids[0]).await.0, StatusCode::NOT_FOUND);
        for id in &ids[1..] {
            assert_eq!(push(&h, id).await.0, StatusCode::ACCEPTED);
        }
    }
}
