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
use serde::{
    Deserialize,
    Serialize,
};
use tauri::AppHandle;
use tauri_plugin_pipedash_push::PipedashPushExt;

use crate::commands::ErrorResponse;
use crate::credential;
use crate::local_notifications::{
    self,
    AlertHost,
};
use crate::remote::alerts::host_request;

const UNAVAILABLE: &str = "Push alerts aren't available in this build.";
const KEYCHAIN_SERVICE: &str = "pipedash";
const KEYCHAIN_ACCOUNT: &str = "device_push";
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

async fn revoke_relay(subscription_id: &str) {
    if let Err(error) = relay::delete(relay::base_url(), subscription_id).await {
        tracing::warn!(%error, "could not revoke the push relay subscription");
    }
}

#[derive(Serialize, Deserialize)]
struct Registration {
    subscription_id: String,
    device_id: String,
}

fn keychain(error: impl std::fmt::Display) -> PushError {
    tracing::warn!(%error, "push keychain access failed");
    PushError::Keychain
}

fn entry() -> Result<credential::Entry> {
    credential::device_only_entry(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).map_err(keychain)
}

fn registration() -> Result<Option<Registration>> {
    match entry()?.get_password() {
        Ok(saved) => Ok(serde_json::from_str(&saved).ok()),
        Err(credential::Error::NoEntry) => Ok(None),
        Err(error) => Err(keychain(error)),
    }
}

fn save_registration(registration: &Registration) -> Result<()> {
    let saved = serde_json::to_string(registration).map_err(keychain)?;
    entry()?.set_password(&saved).map_err(keychain)
}

fn delete_registration() -> Result<()> {
    match entry()?.delete_credential() {
        Ok(()) | Err(credential::Error::NoEntry) => Ok(()),
        Err(error) => Err(keychain(error)),
    }
}

#[cfg(target_os = "ios")]
async fn permission_granted(app: &AppHandle) -> bool {
    use tauri_plugin_notification::NotificationExt;

    let app = app.clone();
    matches!(
        tauri::async_runtime::spawn_blocking(move || app.notification().permission_state()).await,
        Ok(Ok(tauri::plugin::PermissionState::Granted))
    )
}

#[cfg(not(target_os = "ios"))]
async fn permission_granted(_app: &AppHandle) -> bool {
    false
}

#[cfg(target_os = "ios")]
async fn acquire_device_token(app: &AppHandle) -> Result<String> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.pipedash_push().register_for_remote_notifications()
    })
    .await
    .map_err(|_| PushError::Registration)?
    .map_err(PushError::from)
}

#[cfg(not(target_os = "ios"))]
async fn acquire_device_token(_app: &AppHandle) -> Result<String> {
    Err(PushError::Unsupported)
}

async fn available(app: &AppHandle) -> bool {
    cfg!(target_os = "ios") && matches!(app.pipedash_push().is_enabled().await, Ok(true))
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

enum RegisterError {
    HostStopped,
    Failed(String),
}

impl RegisterError {
    fn problem(self, host: &AlertHost) -> String {
        match self {
            Self::HostStopped => no_push_problem(host),
            Self::Failed(problem) => problem,
        }
    }
}

fn no_push_problem(host: &AlertHost) -> String {
    format!(
        "{} doesn't send push alerts. Its owner can turn them on in Settings → Devices.",
        host.host_name
    )
}

fn failed(error: impl std::fmt::Display) -> RegisterError {
    RegisterError::Failed(error.to_string())
}

async fn register(
    host: &AlertHost, device_token: &str, saved: Option<Registration>,
) -> Result<(), RegisterError> {
    let base = relay::base_url();
    let reused = match saved {
        Some(saved) if saved.device_id == host.device_id => Some(saved.subscription_id),
        Some(other) => {
            revoke_relay(&other.subscription_id).await;
            None
        }
        None => None,
    };
    let rotated = match reused {
        Some(subscription_id) => match relay::rotate(base, &subscription_id, device_token).await {
            Ok(()) => Some(subscription_id),
            Err(PushError::RelayGone) => None,
            Err(error) => return Err(failed(error)),
        },
        None => None,
    };
    let fresh = rotated.is_none();
    let subscription_id = match rotated {
        Some(subscription_id) => subscription_id,
        None => relay::create(base, device_token).await.map_err(failed)?,
    };

    let body = serde_json::json!({ "subscription_id": subscription_id }).to_string();
    let accepted = match host_request(host, "PUT", HOST_PUSH_PATH, Some(&body)).await {
        Ok((204, _)) => Ok(()),
        Ok((410, _)) => Err(RegisterError::HostStopped),
        Ok((status, body)) => {
            if host_failure(&body).code.as_deref() == Some(RELAY_NOT_CONFIGURED_CODE) {
                Err(RegisterError::HostStopped)
            } else {
                Err(RegisterError::Failed(host_problem(host, status, &body)))
            }
        }
        Err(error) => Err(failed(error)),
    };
    let saved = accepted.and_then(|()| {
        save_registration(&Registration {
            subscription_id: subscription_id.clone(),
            device_id: host.device_id.clone(),
        })
        .map_err(failed)
    });
    if let Err(error) = saved {
        if fresh {
            revoke_relay(&subscription_id).await;
        }
        return Err(error);
    }
    local_notifications::mark_alert_host_push(host, true).await;

    Ok(())
}

#[derive(Deserialize)]
struct HostPushBody {
    relay_url: Option<String>,
    #[serde(default)]
    enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostPush {
    Active,
    NoRegistration,
    RelayStopped,
}

fn classify(body: &HostPushBody) -> HostPush {
    match body
        .relay_url
        .as_deref()
        .map(|url| url.trim_end_matches('/'))
    {
        Some(url) if url == relay::base_url() && body.enabled => HostPush::Active,
        Some(url) if url == relay::base_url() => HostPush::NoRegistration,
        _ => HostPush::RelayStopped,
    }
}

pub(crate) async fn host_push(host: &AlertHost) -> Option<HostPush> {
    let (status, body) = host_request(host, "GET", HOST_PUSH_PATH, None).await.ok()?;
    if status != 200 {
        return None;
    }

    serde_json::from_str::<HostPushBody>(&body)
        .ok()
        .map(|body| classify(&body))
}

pub async fn enable(app: &AppHandle) -> Result<(), String> {
    if !available(app).await {
        return Err(UNAVAILABLE.to_owned());
    }
    let host = local_notifications::current_alert_host()
        .await
        .ok_or_else(|| "Turn on notifications first.".to_owned())?;

    let (status, body) = host_request(&host, "GET", HOST_PUSH_PATH, None)
        .await
        .map_err(|error| error.to_string())?;
    if status != 200 {
        return Err(host_problem(&host, status, &body));
    }
    let relay_url = serde_json::from_str::<HostPushBody>(&body)
        .ok()
        .and_then(|push| push.relay_url)
        .ok_or_else(|| no_push_problem(&host))?;
    if relay_url.trim_end_matches('/') != relay::base_url() {
        return Err(format!(
            "{} sends push alerts through a different relay.",
            host.host_name
        ));
    }

    let device_token = acquire_device_token(app)
        .await
        .map_err(|error| error.to_string())?;
    register(&host, &device_token, registration().ok().flatten())
        .await
        .map_err(|error| error.problem(&host))
}

pub async fn forget() {
    if !cfg!(target_os = "ios") {
        return;
    }
    match registration() {
        Ok(Some(saved)) => revoke_relay(&saved.subscription_id).await,
        Ok(None) => {}
        Err(error) => tracing::warn!(%error, "could not read the push registration"),
    }
    if let Err(error) = delete_registration() {
        tracing::warn!(%error, "could not remove the push registration");
    }
}

pub async fn disable() {
    if !cfg!(target_os = "ios") {
        return;
    }
    let host = local_notifications::current_alert_host().await;
    if let Some(host) = &host {
        if let Err(error) = host_request(host, "DELETE", HOST_PUSH_PATH, None).await {
            tracing::debug!(%error, "Couldn't tell the alert host to stop push alerts");
        }
    }
    forget().await;
    if let Some(host) = &host {
        local_notifications::mark_alert_host_push(host, false).await;
    }
}

async fn host_stopped(app: &AppHandle, host: &AlertHost) {
    forget().await;
    local_notifications::mark_push_stopped(app, host).await;
}

pub async fn sync_token(app: &AppHandle) {
    if !available(app).await || !permission_granted(app).await {
        return;
    }
    let Some(host) = local_notifications::current_alert_host()
        .await
        .filter(|host| host.push)
    else {
        return;
    };
    if host_push(&host).await == Some(HostPush::RelayStopped) {
        host_stopped(app, &host).await;
        return;
    }
    let device_token = match acquire_device_token(app).await {
        Ok(token) => token,
        Err(error) => {
            tracing::debug!(%error, "Couldn't read the push token");
            return;
        }
    };
    match register(&host, &device_token, registration().ok().flatten()).await {
        Ok(()) => {}
        Err(RegisterError::HostStopped) => host_stopped(app, &host).await,
        Err(RegisterError::Failed(_)) => tracing::debug!("Couldn't refresh the push registration"),
    }
}

pub fn relay_url() -> &'static str {
    relay::base_url()
}

pub fn spawn_sync_token(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { sync_token(&app).await });
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

    async fn paired(relay: Option<&str>) -> (crate::remote::test_host::Host, AlertHost) {
        let host = crate::remote::test_host::start_host_with_relay(relay).await;
        let code = pipedash_web::pairing::create_pairing(&host.state, &host.url.url)
            .unwrap()
            .code;
        let alert_host = crate::remote::alerts::claim_alert_host(host.url.clone(), &code, true)
            .await
            .unwrap();

        (host, alert_host)
    }

    #[tokio::test]
    async fn a_host_that_dropped_its_relay_is_told_apart_from_one_that_still_pushes() {
        let (_off, off_host) = paired(None).await;
        assert_eq!(host_push(&off_host).await, Some(HostPush::RelayStopped));

        let (_other, other_host) = paired(Some("https://relay.example.com")).await;
        assert_eq!(host_push(&other_host).await, Some(HostPush::RelayStopped));

        let (on, on_host) = paired(Some(&format!("{}/", relay::base_url()))).await;
        assert_eq!(host_push(&on_host).await, Some(HostPush::NoRegistration));

        on.core
            .device_pushes()
            .set(&on_host.device_id, "sub-1")
            .await
            .unwrap();
        assert_eq!(host_push(&on_host).await, Some(HostPush::Active));

        let unreachable = AlertHost {
            server_url: "http://127.0.0.1:1".to_owned(),
            ..on_host
        };
        assert_eq!(host_push(&unreachable).await, None);
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
}
