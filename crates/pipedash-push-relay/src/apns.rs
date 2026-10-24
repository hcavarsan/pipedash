use std::sync::{
    Mutex,
    PoisonError,
};
use std::time::{
    Duration,
    SystemTime,
    UNIX_EPOCH,
};

use base64::engine::general_purpose::{
    STANDARD,
    URL_SAFE_NO_PAD,
};
use base64::Engine as _;
use ring::rand::SystemRandom;
use ring::signature::{
    EcdsaKeyPair,
    ECDSA_P256_SHA256_FIXED_SIGNING,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::store::{
    Environment,
    Subscription,
};

pub const PRODUCTION_URL: &str = "https://api.push.apple.com";
pub const SANDBOX_URL: &str = "https://api.sandbox.push.apple.com";

const TOKEN_MAX_AGE_SECS: u64 = 50 * 60;
const _: () = assert!(TOKEN_MAX_AGE_SECS < 60 * 60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("the APNs key is not valid PEM")]
    Pem,
    #[error("the APNs key is not a PKCS#8 ES256 (P-256) private key")]
    Pkcs8,
}

struct CachedToken {
    token: String,
    issued_at: u64,
}

pub struct Signer {
    key_pair: EcdsaKeyPair,
    rng: SystemRandom,
    header: String,
    team_id: String,
    cached: Mutex<Option<CachedToken>>,
}

impl Signer {
    pub fn new(pem: &str, key_id: &str, team_id: &str) -> Result<Self, KeyError> {
        let encoded: String = pem
            .replace("\\n", "\n")
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with("-----"))
            .collect();
        let der = STANDARD.decode(encoded).map_err(|_| KeyError::Pem)?;
        let rng = SystemRandom::new();
        let key_pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &der, &rng)
            .map_err(|_| KeyError::Pkcs8)?;
        let header = URL_SAFE_NO_PAD
            .encode(serde_json::json!({ "alg": "ES256", "kid": key_id }).to_string());

        Ok(Self {
            key_pair,
            rng,
            header,
            team_id: team_id.to_owned(),
            cached: Mutex::new(None),
        })
    }

    pub fn token(&self, now: u64) -> Result<String, ring::error::Unspecified> {
        let mut cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(cached) = cached.as_ref() {
            if now.saturating_sub(cached.issued_at) < TOKEN_MAX_AGE_SECS {
                return Ok(cached.token.clone());
            }
        }

        let token = self.sign(now)?;
        *cached = Some(CachedToken {
            token: token.clone(),
            issued_at: now,
        });

        Ok(token)
    }

    pub fn invalidate(&self) {
        *self.cached.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    fn sign(&self, issued_at: u64) -> Result<String, ring::error::Unspecified> {
        let claims = URL_SAFE_NO_PAD
            .encode(serde_json::json!({ "iss": self.team_id, "iat": issued_at }).to_string());
        let signing_input = format!("{}.{claims}", self.header);
        let signature = self.key_pair.sign(&self.rng, signing_input.as_bytes())?;

        Ok(format!(
            "{signing_input}.{}",
            URL_SAFE_NO_PAD.encode(signature.as_ref())
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Delivered,
    Unregistered,
    Failed,
}

pub fn classify(status: u16, reason: Option<&str>) -> Outcome {
    match (status, reason) {
        (200, _) => Outcome::Delivered,
        (410, _) | (_, Some("BadDeviceToken" | "Unregistered")) => Outcome::Unregistered,
        _ => Outcome::Failed,
    }
}

pub struct Notification<'a> {
    pub title: &'a str,
    pub body: &'a str,
    pub thread_id: Option<&'a str>,
}

#[derive(Serialize)]
struct Payload<'a> {
    aps: Aps<'a>,
}

#[derive(Serialize)]
struct Aps<'a> {
    alert: Alert<'a>,
    sound: &'static str,
    #[serde(rename = "thread-id", skip_serializing_if = "Option::is_none")]
    thread_id: Option<&'a str>,
}

#[derive(Serialize)]
struct Alert<'a> {
    title: &'a str,
    body: &'a str,
}

#[derive(Deserialize)]
struct ErrorBody {
    reason: String,
}

pub struct Apns {
    http: reqwest::Client,
    signer: Signer,
    production_url: String,
    sandbox_url: String,
}

impl Apns {
    pub fn new(signer: Signer, production_url: &str, sandbox_url: &str) -> reqwest::Result<Self> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .tls_backend_rustls()
            .http2_prior_knowledge()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            .build()?;

        Ok(Self {
            http,
            signer,
            production_url: production_url.trim_end_matches('/').to_owned(),
            sandbox_url: sandbox_url.trim_end_matches('/').to_owned(),
        })
    }

    pub async fn send(
        &self, subscription: &Subscription, notification: &Notification<'_>,
    ) -> Outcome {
        let base = match subscription.environment {
            Environment::Production => &self.production_url,
            Environment::Sandbox => &self.sandbox_url,
        };
        let Ok(token) = self.signer.token(unix_now()) else {
            tracing::error!("Couldn't sign the APNs provider token");
            return Outcome::Failed;
        };
        let payload = Payload {
            aps: Aps {
                alert: Alert {
                    title: notification.title,
                    body: notification.body,
                },
                sound: "default",
                thread_id: notification.thread_id,
            },
        };

        let response = self
            .http
            .post(format!("{base}/3/device/{}", subscription.device_token))
            .bearer_auth(token)
            .header("apns-topic", &subscription.bundle_id)
            .header("apns-push-type", "alert")
            .header("apns-priority", "10")
            .json(&payload)
            .send()
            .await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!(error = %error.without_url(), "APNs request failed");
                return Outcome::Failed;
            }
        };

        let status = response.status().as_u16();
        let reason = if status == 200 {
            None
        } else {
            response
                .json::<ErrorBody>()
                .await
                .ok()
                .map(|body| body.reason)
        };
        let outcome = classify(status, reason.as_deref());

        if outcome == Outcome::Failed {
            if reason.as_deref() == Some("ExpiredProviderToken") {
                self.signer.invalidate();
            }
            tracing::warn!(
                status,
                reason = reason.as_deref().unwrap_or(""),
                "APNs rejected a notification"
            );
        }

        outcome
    }
}

#[cfg(test)]
mod tests {
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
                &Notification {
                    title: "build failed",
                    body: "GitHub · main · #7",
                    thread_id: Some("github__1__build"),
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
                    "alert": { "title": "build failed", "body": "GitHub · main · #7" },
                    "sound": "default",
                    "thread-id": "github__1__build",
                }
            })
        );
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
                &Notification {
                    title: "t",
                    body: "b",
                    thread_id: None,
                },
            )
            .await;

        assert_eq!(outcome, Outcome::Failed);
    }
}
