pub(crate) mod flow;
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

use crate::error::ErrorResponse;
use crate::local_notifications::settings::current_alert_host;
use crate::local_notifications::store::AlertHost;
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

    flow::enable_with(&flow::platform::AppEnv(app)).await
}

pub async fn disable(app: &AppHandle) {
    if !cfg!(target_os = "ios") {
        return;
    }
    let _ops = flow::PUSH_OPS.lock().await;
    flow::disable_with(&flow::platform::AppEnv(app)).await;
}

pub async fn forget() {
    if !cfg!(target_os = "ios") {
        return;
    }
    let _ops = flow::PUSH_OPS.lock().await;
    flow::forget_at(relay::base_url()).await;
}

pub async fn sync_token(app: &AppHandle) {
    flow::sync_with(&flow::platform::AppEnv(app)).await;
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
    let host = current_alert_host().await.ok_or_else(|| ErrorResponse {
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

async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| PushError::Storage)?
}

async fn cleanup_legacy(dir: &Path, relay_url: &str) {
    let read_dir = dir.to_path_buf();
    let Ok(file) = blocking(move || store::read(&read_dir)).await else {
        return;
    };
    let mut cleaned = true;
    for server in &file.servers {
        let id = server.id.clone();
        let subscription = match blocking(move || {
            store::get_secret(store::Secret::RelaySubscription, &id)
        })
        .await
        {
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
        let id = server.id.clone();
        if blocking(move || store::forget(&id)).await.is_err() {
            cleaned = false;
        }
    }
    if cleaned {
        let remove_dir = dir.to_path_buf();
        if let Err(error) = blocking(move || store::remove_file(&remove_dir)).await {
            tracing::warn!(%error, "could not remove the old push settings");
        }
    }
}

pub fn spawn_legacy_cleanup(app_data_dir: &Path) {
    let dir = app_data_dir.to_path_buf();
    tauri::async_runtime::spawn(async move { cleanup_legacy(&dir, relay::base_url()).await });
}

#[cfg(test)]
mod tests;
