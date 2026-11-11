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

async fn call(app: &Router, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
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
            "body": "Stub · main · #7",
            "thread_id": "stub__1__build",
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
            "alert": { "title": "build failed", "body": "Stub · main · #7" },
            "sound": "default",
            "thread-id": "stub__1__build",
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
async fn an_alert_id_notification_sends_the_generic_alert_with_collapse_and_expiry() {
    let h = harness().await;
    let id = subscribe(&h, "production", &token("ab")).await;

    let (status, _) = call(
        &h.app,
        Method::POST,
        "/v1/notifications",
        Some(json!({ "subscription_id": id, "alert_id": "0123abcd_-" })),
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED);
    let requests = h.production.take_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].push_type.as_deref(), Some("alert"));
    assert_eq!(requests[0].priority.as_deref(), Some("10"));
    assert_eq!(requests[0].collapse_id.as_deref(), Some("0123abcd_-"));
    let expiration: u64 = requests[0].expiration.as_deref().unwrap().parse().unwrap();
    let now = unix_now();
    assert!((now + 86_400 - 5..=now + 86_400).contains(&expiration));
    assert_eq!(
        requests[0].body,
        json!({
            "aps": {
                "alert": {
                    "title": "Pipedash",
                    "body": "New alert. Open Pipedash to see it.",
                },
                "sound": "default",
                "mutable-content": 1,
                "thread-id": "pipedash-alerts",
            },
            "pipedash": { "alertId": "0123abcd_-" },
        })
    );
}

#[tokio::test]
async fn mixed_empty_and_malformed_notification_modes_are_refused() {
    let h = harness().await;
    let id = subscribe(&h, "production", &token("ab")).await;
    let too_long = "a".repeat(65);
    let bodies = [
        json!({ "subscription_id": id }),
        json!({ "subscription_id": id, "title": "t" }),
        json!({ "subscription_id": id, "alert_id": "ab", "title": "t", "body": "b" }),
        json!({ "subscription_id": id, "alert_id": "ab", "title": "t" }),
        json!({ "subscription_id": id, "alert_id": "ab", "body": "b" }),
        json!({ "subscription_id": id, "alert_id": "ab", "thread_id": "x" }),
        json!({ "subscription_id": id, "alert_id": "ab", "pipeline_id": "x" }),
        json!({ "subscription_id": id, "alert_id": "ab", "provider_id": 1 }),
        json!({ "subscription_id": id, "alert_id": "ab", "run_number": 1 }),
        json!({ "subscription_id": id, "alert_id": "" }),
        json!({ "subscription_id": id, "alert_id": too_long }),
        json!({ "subscription_id": id, "alert_id": "a b" }),
        json!({ "subscription_id": id, "alert_id": "a/b" }),
    ];

    for body in bodies {
        let (status, _) = call(
            &h.app,
            Method::POST,
            "/v1/notifications",
            Some(body.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    }
    assert!(h.production.take_requests().is_empty());
}

#[tokio::test]
async fn a_legacy_notification_has_no_collapse_id_or_expiration() {
    let h = harness().await;
    let id = subscribe(&h, "production", &token("ab")).await;

    assert_eq!(push(&h, &id).await.0, StatusCode::ACCEPTED);

    let requests = h.production.take_requests();
    assert_eq!(requests[0].collapse_id, None);
    assert_eq!(requests[0].expiration, None);
    assert_eq!(requests[0].body["aps"].get("mutable-content"), None);
}

#[tokio::test]
async fn a_token_rotated_during_an_apns_rejection_keeps_the_subscription() {
    let h = harness().await;
    let id = subscribe(&h, "production", &token("ab")).await;
    h.production
        .reply_with(410, &json!({ "reason": "Unregistered" }).to_string());
    let (arrived, release) = h.production.hold_replies();
    let pending = tokio::spawn({
        let app = h.app.clone();
        let id = id.clone();
        async move {
            call(
                &app,
                Method::POST,
                "/v1/notifications",
                Some(json!({ "subscription_id": id, "title": "t", "body": "b" })),
            )
            .await
        }
    });
    arrived.notified().await;

    let (status, _) = call(
        &h.app,
        Method::PUT,
        &format!("/v1/subscriptions/{id}"),
        Some(json!({ "device_token": token("ef") })),
    )
    .await;
    release.notify_one();

    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(pending.await.unwrap().0, StatusCode::BAD_GATEWAY);
    assert_eq!(
        h.store
            .get(&subscription_key(&id))
            .await
            .unwrap()
            .map(|subscription| subscription.device_token),
        Some(token("ef"))
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
async fn a_failure_notification_forwards_its_destination_to_apns() {
    let h = harness().await;
    let id = subscribe(&h, "production", &token("ab")).await;

    let (status, _) = call(
        &h.app,
        Method::POST,
        "/v1/notifications",
        Some(json!({
            "subscription_id": id,
            "title": "build failed",
            "body": "Stub · main · #7",
            "pipeline_id": "stub__1__build",
            "provider_id": 1,
            "run_number": 7,
        })),
    )
    .await;

    assert_eq!(status, StatusCode::ACCEPTED);
    let requests = h.production.take_requests();
    assert_eq!(requests[0].body["aps"]["category"], "pipedash-failure");
    assert_eq!(
        requests[0].body["pipedash"],
        json!({
            "key": "pipeline:stub__1__build",
            "pipelineId": "stub__1__build",
            "providerId": 1,
            "runNumber": 7,
        })
    );
}

#[tokio::test]
async fn an_empty_or_oversized_pipeline_id_is_refused() {
    let h = harness().await;
    let id = subscribe(&h, "production", &token("ab")).await;

    for pipeline_id in [String::new(), "p".repeat(MAX_THREAD_ID_BYTES + 1)] {
        let (status, _) = call(
            &h.app,
            Method::POST,
            "/v1/notifications",
            Some(json!({
                "subscription_id": id,
                "title": "t",
                "body": "b",
                "pipeline_id": pipeline_id,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
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
