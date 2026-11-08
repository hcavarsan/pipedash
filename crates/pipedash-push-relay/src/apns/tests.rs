use axum::http::Version;
use ring::signature::{
    UnparsedPublicKey,
    ECDSA_P256_SHA256_FIXED,
};

use super::*;
use crate::test_support::{
    key_pem,
    StubApns,
};

fn decode(part: &str) -> Vec<u8> {
    URL_SAFE_NO_PAD.decode(part).unwrap()
}

#[test]
fn the_provider_token_has_the_apns_shape_and_a_valid_signature() {
    let (pem, public_key) = key_pem();
    let signer = Signer::new(&pem, "ABC123DEFG", "TEAM123456").unwrap();

    let token = signer.token(1_700_000_000).unwrap();
    let parts: Vec<&str> = token.split('.').collect();

    assert_eq!(parts.len(), 3);
    let header: serde_json::Value = serde_json::from_slice(&decode(parts[0])).unwrap();
    let claims: serde_json::Value = serde_json::from_slice(&decode(parts[1])).unwrap();
    assert_eq!(
        header,
        serde_json::json!({ "alg": "ES256", "kid": "ABC123DEFG" })
    );
    assert_eq!(
        claims,
        serde_json::json!({ "iss": "TEAM123456", "iat": 1_700_000_000 })
    );
    let signature = decode(parts[2]);
    assert_eq!(signature.len(), 64);
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, public_key)
        .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
        .unwrap();
}

#[test]
fn the_token_is_reused_until_it_is_close_to_an_hour_old() {
    let (pem, _) = key_pem();
    let signer = Signer::new(&pem, "K", "T").unwrap();
    let issued = 1_700_000_000;

    let first = signer.token(issued).unwrap();
    assert_eq!(
        signer.token(issued + TOKEN_MAX_AGE_SECS - 1).unwrap(),
        first
    );

    let refreshed = signer.token(issued + TOKEN_MAX_AGE_SECS).unwrap();
    assert_ne!(refreshed, first);

    signer.invalidate();
    assert_ne!(
        signer.token(issued + TOKEN_MAX_AGE_SECS).unwrap(),
        refreshed
    );
}

#[test]
fn keys_that_are_not_es256_pkcs8_pem_are_rejected() {
    assert!(matches!(
        Signer::new(
            "-----BEGIN PRIVATE KEY-----\n!!!\n-----END PRIVATE KEY-----",
            "K",
            "T"
        ),
        Err(KeyError::Pem)
    ));
    assert!(matches!(
        Signer::new(
            "-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----",
            "K",
            "T"
        ),
        Err(KeyError::Pkcs8)
    ));
}

#[test]
fn a_key_with_escaped_newlines_loads_like_a_multi_line_one() {
    let (pem, _) = key_pem();
    let escaped = pem.trim_end().replace('\n', "\\n");

    assert!(Signer::new(&escaped, "K", "T").is_ok());
}

#[test]
fn apns_responses_map_to_outcomes() {
    assert_eq!(classify(200, None), Outcome::Delivered);
    assert_eq!(classify(410, None), Outcome::Unregistered);
    assert_eq!(classify(410, Some("Unregistered")), Outcome::Unregistered);
    assert_eq!(classify(400, Some("BadDeviceToken")), Outcome::Unregistered);
    assert_eq!(classify(400, Some("BadTopic")), Outcome::Failed);
    assert_eq!(classify(403, Some("ExpiredProviderToken")), Outcome::Failed);
    assert_eq!(classify(429, Some("TooManyRequests")), Outcome::Failed);
    assert_eq!(classify(500, None), Outcome::Failed);
    assert_eq!(classify(503, Some("ServiceUnavailable")), Outcome::Failed);
}

#[tokio::test]
async fn a_notification_goes_out_over_http2_with_the_apns_headers_and_payload() {
    let stub = StubApns::start().await;
    let (pem, _) = key_pem();
    let apns = Apns::new(Signer::new(&pem, "K", "T").unwrap(), &stub.url, &stub.url).unwrap();
    let subscription = Subscription {
        device_token: "abcd1234".to_string(),
        environment: Environment::Production,
        bundle_id: "com.example.app".to_string(),
    };

    let outcome = apns
        .send(
            &subscription,
            &Notification::Legacy {
                title: "build failed",
                body: "Stub · main · #7",
                thread_id: Some("stub__1__build"),
                pipeline_id: None,
                provider_id: None,
                run_number: None,
            },
        )
        .await;

    assert_eq!(outcome, Outcome::Delivered);
    let requests = stub.take_requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.version, Version::HTTP_2);
    assert_eq!(request.path, "/3/device/abcd1234");
    assert_eq!(request.topic.as_deref(), Some("com.example.app"));
    assert_eq!(request.push_type.as_deref(), Some("alert"));
    assert!(request
        .authorization
        .as_deref()
        .is_some_and(|value| value.starts_with("Bearer ") && value.matches('.').count() == 2));
    assert_eq!(
        request.body,
        serde_json::json!({
            "aps": {
                "alert": { "title": "build failed", "body": "Stub · main · #7" },
                "sound": "default",
                "thread-id": "stub__1__build",
            }
        })
    );
}

#[test]
fn a_failure_payload_carries_the_destination_and_the_open_run_category() {
    let notification = Notification::Legacy {
        title: "build failed",
        body: "Stub · main · #7",
        thread_id: Some("stub__1__acme/app/ci.yml"),
        pipeline_id: Some("stub__1__acme/app/ci.yml"),
        provider_id: Some(1),
        run_number: Some(7),
    };

    assert_eq!(
        serde_json::to_value(payload(&notification)).unwrap(),
        serde_json::json!({
            "aps": {
                "alert": { "title": "build failed", "body": "Stub · main · #7" },
                "sound": "default",
                "thread-id": "stub__1__acme/app/ci.yml",
                "category": "pipedash-failure",
            },
            "pipedash": {
                "key": "pipeline:stub__1__acme/app/ci.yml",
                "pipelineId": "stub__1__acme/app/ci.yml",
                "providerId": 1,
                "runNumber": 7,
            },
        })
    );
}

#[test]
fn a_notification_without_a_pipeline_has_no_destination_or_category() {
    let notification = Notification::Legacy {
        title: "t",
        body: "b",
        thread_id: None,
        pipeline_id: None,
        provider_id: Some(1),
        run_number: Some(7),
    };

    assert_eq!(
        serde_json::to_value(payload(&notification)).unwrap(),
        serde_json::json!({
            "aps": { "alert": { "title": "t", "body": "b" }, "sound": "default" }
        })
    );
}

#[test]
fn an_alert_payload_is_generic_and_carries_only_the_alert_id() {
    let notification = Notification::Alert { alert_id: "ab12_-" };

    assert_eq!(
        serde_json::to_value(payload(&notification)).unwrap(),
        serde_json::json!({
            "aps": {
                "alert": {
                    "title": "Pipedash",
                    "body": "New alert. Open Pipedash to see it.",
                },
                "sound": "default",
                "mutable-content": 1,
                "thread-id": "pipedash-alerts",
            },
            "pipedash": { "alertId": "ab12_-" },
        })
    );
}

#[tokio::test]
async fn an_alert_goes_out_with_the_collapse_id_and_a_day_of_expiration() {
    let stub = StubApns::start().await;
    let (pem, _) = key_pem();
    let apns = Apns::new(Signer::new(&pem, "K", "T").unwrap(), &stub.url, &stub.url).unwrap();
    let subscription = Subscription {
        device_token: "abcd1234".to_string(),
        environment: Environment::Production,
        bundle_id: "com.example.app".to_string(),
    };

    let before = unix_now();
    let outcome = apns
        .send(&subscription, &Notification::Alert { alert_id: "ab12" })
        .await;
    let after = unix_now();

    assert_eq!(outcome, Outcome::Delivered);
    let requests = stub.take_requests();
    assert_eq!(requests[0].push_type.as_deref(), Some("alert"));
    assert_eq!(requests[0].priority.as_deref(), Some("10"));
    assert_eq!(requests[0].collapse_id.as_deref(), Some("ab12"));
    let expiration: u64 = requests[0].expiration.as_deref().unwrap().parse().unwrap();
    assert!((before + ALERT_TTL_SECS..=after + ALERT_TTL_SECS).contains(&expiration));
}

#[tokio::test]
async fn an_unreachable_apns_is_a_failure_not_a_removal() {
    let (pem, _) = key_pem();
    let apns = Apns::new(
        Signer::new(&pem, "K", "T").unwrap(),
        "http://127.0.0.1:1",
        "http://127.0.0.1:1",
    )
    .unwrap();
    let subscription = Subscription {
        device_token: "abcd1234".to_string(),
        environment: Environment::Sandbox,
        bundle_id: "com.example.app".to_string(),
    };

    let outcome = apns
        .send(
            &subscription,
            &Notification::Legacy {
                title: "t",
                body: "b",
                thread_id: None,
                pipeline_id: None,
                provider_id: None,
                run_number: None,
            },
        )
        .await;

    assert_eq!(outcome, Outcome::Failed);
}
