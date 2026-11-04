mod flow;
mod relay;
mod store;
#[cfg(test)]
mod stub;

use std::path::Path;
use std::sync::{
    Arc,
    OnceLock,
};

use pipedash_core::infrastructure::HttpClientManager;
use pipedash_plugin_api::ClientOptions;
use reqwest::{
    RequestBuilder,
    Response,
    StatusCode,
};
use serde::Deserialize;
use tauri::AppHandle;

pub(crate) use self::flow::{
    host_push,
    HostPush,
};
use crate::commands::ErrorResponse;
use crate::local_notifications::{
    self,
    AlertHost,
};
use crate::remote::alerts::host_request;

const UNAVAILABLE: &str = "Push alerts aren't available in this build.";
const HOST_PUSH_PATH: &str = "/devices/current/push";
const RELAY_NOT_CONFIGURED_CODE: &str = "PUSH_RELAY_NOT_CONFIGURED";

type Result<T, E = PushError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
enum PushError {
    #[error("Couldn't reach the push relay.")]
    Unreachable,
    #[error("The push relay answered unexpectedly (HTTP {0}).")]
    Unexpected(u16),
    #[error("The push relay is busy. Try again later.")]
    RateLimited,
    #[error("The push relay no longer knows this device.")]
    RelayGone,
    #[error("Allow notifications for Pipedash in Settings.")]
    PermissionDenied,
    #[error("Push alerts aren't available in this build.")]
    Unsupported,
    #[error("Couldn't register this device for push alerts.")]
    Registration,
    #[error("Couldn't read or write the push settings.")]
    Storage,
    #[error("Couldn't access the system keychain.")]
    Keychain,
    #[error("Couldn't create the HTTP client.")]
    Client,
}

impl From<tauri_plugin_pipedash_push::Error> for PushError {
    fn from(error: tauri_plugin_pipedash_push::Error) -> Self {
        match error {
            tauri_plugin_pipedash_push::Error::PermissionDenied => Self::PermissionDenied,
            tauri_plugin_pipedash_push::Error::Unsupported => Self::Unsupported,
            other => {
                tracing::warn!(error = %other, "push registration failed");
                Self::Registration
            }
        }
    }
}

fn status_error(status: u16) -> PushError {
    match status {
        404 | 410 => PushError::RelayGone,
        429 => PushError::RateLimited,
        500..=599 => PushError::Unreachable,
        _ => PushError::Unexpected(status),
    }
}

fn client() -> Result<&'static reqwest::Client> {
    static CLIENT: OnceLock<Arc<reqwest::Client>> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client);
    }
    let built = HttpClientManager::new()
        .and_then(|manager| manager.client_for(&ClientOptions::default()))
        .map_err(|error| {
            tracing::warn!(%error, "could not build the push HTTP client");
            PushError::Client
        })?;
    Ok(CLIENT.get_or_init(|| built))
}

async fn call(request: RequestBuilder, allow_missing: bool) -> Result<Response> {
    let response = request.send().await.map_err(|_| {
        tracing::warn!("push relay request could not be sent");
        PushError::Unreachable
    })?;
    let status = response.status();
    let missing = status == StatusCode::NOT_FOUND || status == StatusCode::GONE;
    if status.is_success() || (allow_missing && missing) {
        return Ok(response);
    }
    tracing::warn!(status = status.as_u16(), "push relay request rejected");
    Err(status_error(status.as_u16()))
}

#[derive(Default, Deserialize)]
struct HostFailure {
    code: Option<String>,
    message: Option<String>,
}

fn host_failure(body: &str) -> HostFailure {
    serde_json::from_str(body).unwrap_or_default()
}

fn host_problem(host: &AlertHost, status: u16, body: &str) -> String {
    host_failure(body)
        .message
        .unwrap_or_else(|| format!("{} answered unexpectedly (HTTP {status}).", host.host_name))
}

pub async fn enable(app: &AppHandle) -> Result<(), String> {
    let _ops = flow::PUSH_OPS.lock().await;

    flow::enable_with(&flow::AppEnv(app)).await
}

pub async fn disable(app: &AppHandle) {
    if !cfg!(target_os = "ios") {
        return;
    }
    let _ops = flow::PUSH_OPS.lock().await;
    flow::disable_with(&flow::AppEnv(app)).await;
}

pub async fn forget() {
    if !cfg!(target_os = "ios") {
        return;
    }
    let _ops = flow::PUSH_OPS.lock().await;
    flow::forget_at(relay::base_url()).await;
}

pub async fn sync_token(app: &AppHandle) {
    flow::sync_with(&flow::AppEnv(app)).await;
}

pub fn relay_url() -> &'static str {
    relay::base_url()
}

pub fn spawn_sync_token(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { sync_token(&app).await });
}

pub(crate) async fn acknowledge(host: &AlertHost, through: i64) -> bool {
    let body = serde_json::json!({ "through": through }).to_string();

    matches!(
        host_request(host, "POST", "/devices/current/push/ack", Some(&body)).await,
        Ok((204, _))
    )
}

#[tauri::command]
pub async fn send_test_alert() -> Result<(), ErrorResponse> {
    let host = local_notifications::current_alert_host()
        .await
        .ok_or_else(|| ErrorResponse {
            error: "Turn on notifications first.".to_owned(),
            details: None,
            code: None,
        })?;
    let (status, body) = host_request(&host, "POST", "/devices/current/push/test", None).await?;
    if status == 202 {
        return Ok(());
    }
    let failure = host_failure(&body);
    Err(ErrorResponse {
        error: host_problem(&host, status, &body),
        details: None,
        code: failure.code,
    })
}

async fn cleanup_legacy(dir: &Path, relay_url: &str) {
    let Ok(file) = store::read(dir) else {
        return;
    };
    let mut cleaned = true;
    for server in &file.servers {
        let subscription = match store::get_secret(store::Secret::RelaySubscription, &server.id) {
            Ok(subscription) => subscription,
            Err(_) => {
                cleaned = false;
                continue;
            }
        };
        if let Some(subscription_id) = subscription {
            if relay::delete(relay_url, &subscription_id).await.is_err() {
                cleaned = false;
                continue;
            }
        }
        if store::forget(&server.id).is_err() {
            cleaned = false;
        }
    }
    if cleaned {
        if let Err(error) = store::remove_file(dir) {
            tracing::warn!(%error, "could not remove the old push settings");
        }
    }
}

pub fn spawn_legacy_cleanup(app_data_dir: &Path) {
    let dir = app_data_dir.to_path_buf();
    tauri::async_runtime::spawn(async move { cleanup_legacy(&dir, relay::base_url()).await });
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::stub::Stub;
    use super::*;
    use crate::push::store::Secret;

    #[test]
    fn maps_relay_status_codes() {
        assert!(matches!(status_error(404), PushError::RelayGone));
        assert!(matches!(status_error(410), PushError::RelayGone));
        assert!(matches!(status_error(429), PushError::RateLimited));
        assert!(matches!(status_error(503), PushError::Unreachable));
        assert!(matches!(status_error(401), PushError::Unexpected(401)));
    }

    #[tokio::test]
    async fn a_rotated_subscription_the_relay_forgot_is_reported_as_gone() {
        let relay = Stub::start(|request| match request.path.as_str() {
            "/v1/subscriptions/gone" => 410,
            "/v1/subscriptions/down" => 502,
            _ => 204,
        })
        .await;

        assert!(relay::rotate(&relay.url, "live", "token").await.is_ok());
        assert!(matches!(
            relay::rotate(&relay.url, "gone", "token").await,
            Err(PushError::RelayGone)
        ));
        assert!(matches!(
            relay::rotate(&relay.url, "down", "token").await,
            Err(PushError::Unreachable)
        ));
        assert!(relay::delete(&relay.url, "gone").await.is_ok());
        assert!(relay::delete(&relay.url, "down").await.is_err());
    }

    fn seed(dir: &Path, servers: &[(&str, &str)]) {
        for (id, subscription) in servers {
            store::set_secret(Secret::ApiToken, id, "api-token");
            store::set_secret(Secret::RelaySubscription, id, subscription);
        }
        let records: Vec<_> = servers.iter().map(|(id, _)| json!({ "id": id })).collect();
        std::fs::write(
            dir.join("push_servers.json"),
            json!({ "servers": records }).to_string(),
        )
        .unwrap();
    }

    fn secrets(id: &str) -> (Option<String>, Option<String>) {
        (
            store::get_secret(Secret::ApiToken, id).unwrap(),
            store::get_secret(Secret::RelaySubscription, id).unwrap(),
        )
    }

    #[tokio::test]
    async fn legacy_cleanup_keeps_a_record_until_the_relay_confirms() {
        let dir = tempfile::tempdir().unwrap();
        seed(
            dir.path(),
            &[("cleanup-ok", "sub-ok"), ("cleanup-down", "sub-down")],
        );
        let flaky = Stub::start(|request| {
            if request.path.ends_with("sub-down") {
                503
            } else {
                204
            }
        })
        .await;

        cleanup_legacy(dir.path(), &flaky.url).await;

        assert_eq!(secrets("cleanup-ok"), (None, None));
        assert_eq!(
            secrets("cleanup-down"),
            (Some("api-token".to_owned()), Some("sub-down".to_owned()))
        );
        assert!(dir.path().join("push_servers.json").exists());

        let healthy = Stub::start(|_| 204).await;
        cleanup_legacy(dir.path(), &healthy.url).await;

        assert_eq!(secrets("cleanup-down"), (None, None));
        assert!(!dir.path().join("push_servers.json").exists());
        assert_eq!(flaky.count("DELETE", "/v1/subscriptions/sub-ok"), 1);
        assert_eq!(healthy.count("DELETE", "/v1/subscriptions/sub-down"), 1);
        assert_eq!(healthy.count("DELETE", "/v1/subscriptions/sub-ok"), 0);
    }

    #[tokio::test]
    async fn acknowledging_tells_the_host_which_alerts_were_shown() {
        let (host, alert_host) = flow_paired().await;
        host.core
            .device_pushes()
            .set(&alert_host.device_id, "sub-1")
            .await
            .unwrap();

        assert!(acknowledge(&alert_host, 5).await);
        assert!(
            !acknowledge(
                &AlertHost {
                    server_url: "http://127.0.0.1:1".to_owned(),
                    ..alert_host
                },
                5
            )
            .await
        );
    }

    async fn flow_paired() -> (crate::remote::test_host::Host, AlertHost) {
        let host =
            crate::remote::test_host::start_host_with_relay(Some("https://relay.example.com"))
                .await;
        let code = pipedash_web::pairing::create_pairing(&host.state, &host.url.url)
            .unwrap()
            .code;
        let alert_host = crate::remote::alerts::claim_alert_host(host.url.clone(), &code, true)
            .await
            .unwrap();

        (host, alert_host)
    }
}
