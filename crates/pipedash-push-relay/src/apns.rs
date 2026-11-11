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

pub const FAILURE_CATEGORY: &str = "pipedash-failure";
pub const ALERT_TITLE: &str = "Pipedash";
pub const ALERT_BODY: &str = "New alert. Open Pipedash to see it.";
pub const ALERT_THREAD: &str = "pipedash-alerts";
pub const ALERT_TTL_SECS: u64 = 24 * 60 * 60;

pub enum Notification<'a> {
    Legacy {
        title: &'a str,
        body: &'a str,
        thread_id: Option<&'a str>,
        pipeline_id: Option<&'a str>,
        provider_id: Option<u64>,
        run_number: Option<u64>,
    },
    Alert {
        alert_id: &'a str,
    },
}

#[derive(Serialize)]
struct Payload<'a> {
    aps: Aps<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pipedash: Option<Destination<'a>>,
}

#[derive(Serialize)]
struct Aps<'a> {
    alert: Alert<'a>,
    sound: &'static str,
    #[serde(rename = "mutable-content", skip_serializing_if = "Option::is_none")]
    mutable_content: Option<u8>,
    #[serde(rename = "thread-id", skip_serializing_if = "Option::is_none")]
    thread_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<&'static str>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Destination<'a> {
    Pipeline(PipelineDestination<'a>),
    Alert(AlertDestination<'a>),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PipelineDestination<'a> {
    key: String,
    pipeline_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_number: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AlertDestination<'a> {
    alert_id: &'a str,
}

fn payload<'a>(notification: &Notification<'a>) -> Payload<'a> {
    match *notification {
        Notification::Legacy {
            title,
            body,
            thread_id,
            pipeline_id,
            provider_id,
            run_number,
        } => Payload {
            aps: Aps {
                alert: Alert { title, body },
                sound: "default",
                mutable_content: None,
                thread_id,
                category: pipeline_id.map(|_| FAILURE_CATEGORY),
            },
            pipedash: pipeline_id.map(|pipeline_id| {
                Destination::Pipeline(PipelineDestination {
                    key: format!("pipeline:{pipeline_id}"),
                    pipeline_id,
                    provider_id,
                    run_number,
                })
            }),
        },
        Notification::Alert { alert_id } => Payload {
            aps: Aps {
                alert: Alert {
                    title: ALERT_TITLE,
                    body: ALERT_BODY,
                },
                sound: "default",
                mutable_content: Some(1),
                thread_id: Some(ALERT_THREAD),
                category: None,
            },
            pipedash: Some(Destination::Alert(AlertDestination { alert_id })),
        },
    }
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
        let payload = payload(notification);

        let mut request = self
            .http
            .post(format!("{base}/3/device/{}", subscription.device_token))
            .bearer_auth(token)
            .header("apns-topic", &subscription.bundle_id)
            .header("apns-push-type", "alert")
            .header("apns-priority", "10");
        if let Notification::Alert { alert_id } = notification {
            request = request
                .header("apns-collapse-id", *alert_id)
                .header("apns-expiration", (unix_now() + ALERT_TTL_SECS).to_string());
        }
        let response = request.json(&payload).send().await;
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
mod tests;
