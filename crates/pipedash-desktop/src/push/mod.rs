mod relay;
mod server;
mod store;

use std::path::Path;
use std::sync::{
    Arc,
    OnceLock,
};

use chrono::Utc;
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
use tauri::plugin::PermissionState;
use tauri::{
    AppHandle,
    State,
};
use tauri_plugin_pipedash_push::PipedashPushExt;

use self::store::{
    Secret,
    ServerRecord,
};
use crate::commands::ErrorResponse;
use crate::AppDataDir;

const LABEL_MAX_CHARS: usize = 64;
const RELAY_MISSING_CODE: &str = "PUSH_RELAY_NOT_CONFIGURED";
static STATE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

type Result<T, E = PushError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
enum PushError {
    #[error("push relay is not configured in this build of the app")]
    RelayNotConfigured,
    #[error("invalid server URL: {0}")]
    InvalidUrl(&'static str),
    #[error("an API token is required")]
    EmptyToken,
    #[error("the stored API token is missing, pair the server again")]
    TokenMissing,
    #[error("the server rejected the API token")]
    Unauthorized,
    #[error("this server has no push relay configured")]
    ServerRelayMissing,
    #[error("rate limited, try again later")]
    RateLimited,
    #[error("could not reach {0}")]
    Unreachable(String),
    #[error("{0} returned an unexpected response (HTTP {1})")]
    Unexpected(String, u16),
    #[error("unknown push server")]
    UnknownServer,
    #[error("notifications are denied in iOS Settings")]
    PermissionDenied,
    #[error("push notifications are not supported on this device")]
    Unsupported,
    #[error("could not register for push notifications")]
    Registration,
    #[error("could not read or write the push settings")]
    Storage,
    #[error("could not access the system keychain")]
    Keychain,
    #[error("could not create the HTTP client")]
    Client,
    #[error("the push relay no longer knows this device")]
    RelayGone,
}

impl PushError {
    fn code(&self) -> &'static str {
        match self {
            Self::RelayNotConfigured => "RELAY_NOT_CONFIGURED",
            Self::InvalidUrl(_) => "INVALID_URL",
            Self::EmptyToken => "EMPTY_TOKEN",
            Self::TokenMissing => "TOKEN_MISSING",
            Self::Unauthorized => "UNAUTHORIZED",
            Self::ServerRelayMissing => RELAY_MISSING_CODE,
            Self::RateLimited => "RATE_LIMITED",
            Self::Unreachable(_) => "UNREACHABLE",
            Self::Unexpected(..) => "UNEXPECTED_RESPONSE",
            Self::UnknownServer => "UNKNOWN_SERVER",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::Unsupported => "UNSUPPORTED",
            Self::Registration => "REGISTRATION_FAILED",
            Self::Storage => "STORAGE",
            Self::Keychain => "KEYCHAIN",
            Self::Client => "HTTP_CLIENT",
            Self::RelayGone => "RELAY_SUBSCRIPTION_GONE",
        }
    }
}

impl From<PushError> for ErrorResponse {
    fn from(error: PushError) -> Self {
        Self {
            error: error.to_string(),
            details: Some(error.code().to_owned()),
            code: None,
        }
    }
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

enum Peer<'a> {
    Relay,
    Server(&'a str),
}

impl Peer<'_> {
    fn label(&self) -> String {
        match self {
            Self::Relay => "the push relay".to_owned(),
            Self::Server(host) => (*host).to_owned(),
        }
    }
}

#[derive(Deserialize)]
struct ServerErrorBody {
    code: String,
}

fn status_error(peer: &Peer<'_>, status: u16, code: Option<&str>) -> PushError {
    match (peer, status, code) {
        (Peer::Server(_), 401 | 403, _) => PushError::Unauthorized,
        (Peer::Server(_), 422, Some(RELAY_MISSING_CODE)) => PushError::ServerRelayMissing,
        (Peer::Relay, 404 | 410, _) => PushError::RelayGone,
        (_, 429, _) => PushError::RateLimited,
        (_, 500..=599, _) => PushError::Unreachable(peer.label()),
        _ => PushError::Unexpected(peer.label(), status),
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

async fn call(peer: &Peer<'_>, request: RequestBuilder, allow_missing: bool) -> Result<Response> {
    let label = peer.label();
    let response = request.send().await.map_err(|_| {
        tracing::warn!(peer = %label, "push request could not be sent");
        PushError::Unreachable(label.clone())
    })?;
    let status = response.status();
    let missing = status == StatusCode::NOT_FOUND || status == StatusCode::GONE;
    if status.is_success() || (allow_missing && missing) {
        return Ok(response);
    }
    let code = match peer {
        Peer::Server(_) => response
            .json::<ServerErrorBody>()
            .await
            .ok()
            .map(|body| body.code),
        Peer::Relay => None,
    };
    tracing::warn!(peer = %label, status = status.as_u16(), "push request rejected");
    Err(status_error(peer, status.as_u16(), code.as_deref()))
}

#[derive(Debug, PartialEq)]
struct NormalizedUrl {
    url: String,
    host: String,
}

fn normalize_server_url(input: &str) -> Result<NormalizedUrl> {
    let parsed =
        reqwest::Url::parse(input.trim()).map_err(|_| PushError::InvalidUrl("not a valid URL"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(PushError::InvalidUrl("must start with http:// or https://"));
    }
    let host = parsed
        .host_str()
        .filter(|host| !host.is_empty())
        .ok_or(PushError::InvalidUrl("missing host"))?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(PushError::InvalidUrl("must not contain credentials"));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return Err(PushError::InvalidUrl(
            "must not contain a query or fragment",
        ));
    }
    let port = parsed
        .port()
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    let path = parsed.path().trim_end_matches('/');
    Ok(NormalizedUrl {
        url: format!("{}://{host}{port}{path}", parsed.scheme()),
        host: host.to_owned(),
    })
}

fn host_of(url: &str) -> String {
    normalize_server_url(url)
        .map(|normalized| normalized.host)
        .unwrap_or_else(|_| "the server".to_owned())
}

fn default_label(label: Option<&str>, host: &str) -> String {
    label
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .unwrap_or(host)
        .chars()
        .take(LABEL_MAX_CHARS)
        .collect()
}

fn permission_label(state: PermissionState) -> &'static str {
    match state {
        PermissionState::Granted => "granted",
        PermissionState::Denied => "denied",
        PermissionState::Prompt | PermissionState::PromptWithRationale => "prompt",
    }
}

#[cfg(target_os = "ios")]
async fn permission_state(app: &AppHandle) -> PermissionState {
    use tauri_plugin_notification::NotificationExt;

    let app = app.clone();
    match tauri::async_runtime::spawn_blocking(move || app.notification().permission_state()).await
    {
        Ok(Ok(state)) => state,
        _ => PermissionState::Prompt,
    }
}

#[cfg(not(target_os = "ios"))]
async fn permission_state(_app: &AppHandle) -> PermissionState {
    PermissionState::Denied
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

#[derive(Serialize)]
pub struct PushServer {
    pub id: String,
    pub url: String,
    pub label: String,
    pub paired_at: String,
}

#[derive(Serialize)]
pub struct PushStatus {
    pub supported: bool,
    pub relay_configured: bool,
    pub permission: String,
    pub servers: Vec<PushServer>,
}

async fn ensure_enabled(app: &AppHandle) -> Result<()> {
    if !app.pipedash_push().is_enabled().await? {
        return Err(PushError::Unsupported);
    }
    Ok(())
}

async fn status(app: &AppHandle, dir: &Path) -> Result<PushStatus> {
    if !app.pipedash_push().is_enabled().await? {
        return Ok(PushStatus {
            supported: false,
            relay_configured: relay::base_url().is_some(),
            permission: "prompt".to_owned(),
            servers: Vec::new(),
        });
    }
    let file = store::read(dir)?;
    Ok(PushStatus {
        supported: true,
        relay_configured: relay::base_url().is_some(),
        permission: permission_label(permission_state(app).await).to_owned(),
        servers: file
            .servers
            .into_iter()
            .map(|record| PushServer {
                id: record.id,
                url: record.url,
                label: record.label,
                paired_at: record.paired_at.to_rfc3339(),
            })
            .collect(),
    })
}

async fn revoke_relay(base: &str, subscription_id: &str) {
    if let Err(error) = relay::delete(base, subscription_id).await {
        tracing::warn!(%error, "could not revoke the push relay subscription");
    }
}

fn persist(
    dir: &Path, record: &ServerRecord, subscription_id: &str, api_token: &str,
) -> Result<()> {
    store::set_secret(Secret::ApiToken, &record.id, api_token)?;
    store::set_secret(Secret::RelaySubscription, &record.id, subscription_id)?;
    store::update(dir, |file| {
        file.servers.retain(|server| server.url != record.url);
        file.servers.push(record.clone());
    })
}

struct Pairing<'a> {
    url: &'a str,
    host: &'a str,
    api_token: &'a str,
    label: &'a str,
    device_token: &'a str,
}

async fn establish(
    dir: &Path, relay_url: &str, pairing: &Pairing<'_>, stale: Option<&ServerRecord>,
) -> Result<()> {
    let subscription_id = relay::create(relay_url, pairing.device_token).await?;
    let registered = server::register(
        pairing.url,
        pairing.host,
        pairing.api_token,
        &subscription_id,
    )
    .await;
    let id = match registered {
        Ok(id) => id,
        Err(error) => {
            revoke_relay(relay_url, &subscription_id).await;
            return Err(error);
        }
    };
    let record = ServerRecord {
        id,
        url: pairing.url.to_owned(),
        label: pairing.label.to_owned(),
        paired_at: stale.map_or_else(Utc::now, |stale| stale.paired_at),
    };
    if let Err(error) = persist(dir, &record, &subscription_id, pairing.api_token) {
        if let Err(cleanup) =
            server::delete(pairing.url, pairing.host, pairing.api_token, &record.id).await
        {
            tracing::warn!(error = %cleanup, "could not roll back the push server subscription");
        }
        revoke_relay(relay_url, &subscription_id).await;
        if let Err(cleanup) = store::forget(&record.id) {
            tracing::warn!(%cleanup, "could not roll back the saved push credentials");
        }
        return Err(error);
    }
    if let Some(stale) = stale.filter(|stale| stale.id != record.id) {
        if let Err(error) =
            server::delete(pairing.url, pairing.host, pairing.api_token, &stale.id).await
        {
            tracing::warn!(%error, "could not remove the old subscription from the push server");
        }
        store::forget(&stale.id)?;
    }
    Ok(())
}

async fn pair(
    app: &AppHandle, dir: &Path, server_url: &str, api_token: &str, label: Option<&str>,
) -> Result<()> {
    let _state = STATE_LOCK.lock().await;
    let target = normalize_server_url(server_url)?;
    let api_token = api_token.trim();
    if api_token.is_empty() {
        return Err(PushError::EmptyToken);
    }
    let relay_url = relay::base_url().ok_or(PushError::RelayNotConfigured)?;
    let label = default_label(label, &target.host);
    let existing = store::read(dir)?
        .servers
        .into_iter()
        .find(|server| server.url == target.url);
    let device_token = acquire_device_token(app).await?;

    if let Some(record) = &existing {
        if let Some(subscription_id) = store::get_secret(Secret::RelaySubscription, &record.id)? {
            match relay::rotate(relay_url, &subscription_id, &device_token).await {
                Ok(()) => {
                    server::register(&target.url, &target.host, api_token, &subscription_id)
                        .await?;
                    store::set_secret(Secret::ApiToken, &record.id, api_token)?;
                    return store::update(dir, |file| {
                        if let Some(server) = file.servers.iter_mut().find(|s| s.id == record.id) {
                            server.label = label;
                        }
                    });
                }
                Err(PushError::RelayGone) => {
                    tracing::info!("the saved push relay subscription is gone, creating a new one");
                }
                Err(error) => return Err(error),
            }
        }
    }

    let pairing = Pairing {
        url: &target.url,
        host: &target.host,
        api_token,
        label: &label,
        device_token: &device_token,
    };
    establish(dir, relay_url, &pairing, existing.as_ref()).await
}

async fn unpair(dir: &Path, id: &str) -> Result<()> {
    let _state = STATE_LOCK.lock().await;
    let record = store::read(dir)?
        .servers
        .into_iter()
        .find(|server| server.id == id)
        .ok_or(PushError::UnknownServer)?;

    if let Some(subscription_id) = store::get_secret(Secret::RelaySubscription, id)? {
        let relay_url = relay::base_url().ok_or(PushError::RelayNotConfigured)?;
        relay::delete(relay_url, &subscription_id).await?;
    }
    if let Ok(Some(api_token)) = store::get_secret(Secret::ApiToken, id) {
        let host = host_of(&record.url);
        if let Err(error) = server::delete(&record.url, &host, &api_token, id).await {
            tracing::warn!(%error, "could not remove the subscription from the push server");
        }
    }

    store::forget(id)?;
    store::update(dir, |file| file.servers.retain(|server| server.id != id))?;
    Ok(())
}

async fn send_test(dir: &Path, id: &str) -> Result<()> {
    let record = store::read(dir)?
        .servers
        .into_iter()
        .find(|server| server.id == id)
        .ok_or(PushError::UnknownServer)?;
    let api_token = store::get_secret(Secret::ApiToken, id)?.ok_or(PushError::TokenMissing)?;
    server::test(&record.url, &host_of(&record.url), &api_token, id).await
}

#[tauri::command]
pub async fn get_push_status(
    app: AppHandle, app_data_dir: State<'_, AppDataDir>,
) -> Result<PushStatus, ErrorResponse> {
    Ok(status(&app, &app_data_dir.0).await?)
}

#[tauri::command]
pub async fn push_pair_server(
    app: AppHandle, app_data_dir: State<'_, AppDataDir>, server_url: String, api_token: String,
    label: Option<String>,
) -> Result<PushStatus, ErrorResponse> {
    ensure_enabled(&app).await?;
    pair(
        &app,
        &app_data_dir.0,
        &server_url,
        &api_token,
        label.as_deref(),
    )
    .await?;
    Ok(status(&app, &app_data_dir.0).await?)
}

#[tauri::command]
pub async fn push_unpair_server(
    app: AppHandle, app_data_dir: State<'_, AppDataDir>, id: String,
) -> Result<PushStatus, ErrorResponse> {
    ensure_enabled(&app).await?;
    unpair(&app_data_dir.0, &id).await?;
    Ok(status(&app, &app_data_dir.0).await?)
}

#[tauri::command]
pub async fn push_send_test(
    app: AppHandle, app_data_dir: State<'_, AppDataDir>, id: String,
) -> Result<(), ErrorResponse> {
    ensure_enabled(&app).await?;
    Ok(send_test(&app_data_dir.0, &id).await?)
}

#[cfg(any(target_os = "ios", test))]
async fn rotate_or_replace(
    dir: &Path, relay_url: &str, server: &ServerRecord, device_token: &str,
) -> Result<()> {
    if let Some(subscription_id) = store::get_secret(Secret::RelaySubscription, &server.id)? {
        match relay::rotate(relay_url, &subscription_id, device_token).await {
            Err(PushError::RelayGone) => {}
            other => return other,
        }
    }
    let api_token =
        store::get_secret(Secret::ApiToken, &server.id)?.ok_or(PushError::TokenMissing)?;
    let host = host_of(&server.url);
    let pairing = Pairing {
        url: &server.url,
        host: &host,
        api_token: &api_token,
        label: &server.label,
        device_token,
    };
    establish(dir, relay_url, &pairing, Some(server)).await
}

#[cfg(any(target_os = "ios", test))]
async fn sync_servers(dir: &Path, relay_url: &str, device_token: &str) -> Result<()> {
    let _state = STATE_LOCK.lock().await;
    for server in &store::read(dir)?.servers {
        if let Err(error) = rotate_or_replace(dir, relay_url, server, device_token).await {
            tracing::warn!(host = %host_of(&server.url), %error, "push token sync failed");
        }
    }
    Ok(())
}

#[cfg(target_os = "ios")]
async fn sync_token(app: &AppHandle, dir: &Path) -> Result<()> {
    if !app.pipedash_push().is_enabled().await? {
        return Ok(());
    }
    let Some(relay_url) = relay::base_url() else {
        return Ok(());
    };
    if store::read(dir)?.servers.is_empty()
        || !matches!(permission_state(app).await, PermissionState::Granted)
    {
        return Ok(());
    }
    let device_token = acquire_device_token(app).await?;
    sync_servers(dir, relay_url, &device_token).await
}

pub async fn reset(dir: &Path) -> Result<(), ErrorResponse> {
    reset_with(dir, relay::base_url()).await.map_err(Into::into)
}

async fn reset_with(dir: &Path, relay_url: Option<&str>) -> Result<()> {
    let _state = STATE_LOCK.lock().await;
    let servers = store::read(dir)?.servers;
    for record in servers {
        if let (Some(relay_url), Some(subscription_id)) = (
            relay_url,
            store::get_secret(Secret::RelaySubscription, &record.id)?,
        ) {
            revoke_relay(relay_url, &subscription_id).await;
        }
        if let Some(api_token) = store::get_secret(Secret::ApiToken, &record.id)? {
            let host = host_of(&record.url);
            if let Err(error) = server::delete(&record.url, &host, &api_token, &record.id).await {
                tracing::warn!(%error, "could not remove the subscription from the push server");
            }
        }
        store::forget(&record.id)?;
    }
    store::remove_file(dir)
}

pub fn spawn_token_sync(app: &AppHandle, app_data_dir: &Path) {
    #[cfg(target_os = "ios")]
    {
        let app = app.clone();
        let dir = app_data_dir.to_path_buf();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = sync_token(&app, &dir).await {
                tracing::warn!(%error, "push token sync failed");
            }
        });
    }
    #[cfg(not(target_os = "ios"))]
    let _ = (app, app_data_dir);
}

#[cfg(test)]
mod flow_tests;
#[cfg(test)]
mod stub;

#[cfg(test)]
mod tests {
    use super::*;

    fn normalized(input: &str) -> String {
        normalize_server_url(input).unwrap().url
    }

    #[test]
    fn normalizes_accepted_urls() {
        assert_eq!(
            normalized("  HTTPS://CI.Example.COM/  "),
            "https://ci.example.com"
        );
        assert_eq!(normalized("http://10.0.0.5:8080"), "http://10.0.0.5:8080");
        assert_eq!(
            normalized("https://host/Base/path//"),
            "https://host/Base/path"
        );
        assert_eq!(normalized("https://host:443/x/"), "https://host/x");
        assert_eq!(normalized("http://[::1]:3000/"), "http://[::1]:3000");
    }

    #[test]
    fn reports_the_host() {
        let n = normalize_server_url("https://User.example.com:8443/a").unwrap();
        assert_eq!(n.host, "user.example.com");
    }

    #[test]
    fn rejects_unusable_urls() {
        for input in [
            "",
            "   ",
            "ci.example.com",
            "ftp://ci.example.com",
            "mailto:a@b.c",
            "https://",
            "https://user@ci.example.com",
            "https://user:pw@ci.example.com",
            "https://ci.example.com?a=1",
            "https://ci.example.com/#frag",
            "https://ci.example.com/?",
        ] {
            assert!(
                matches!(normalize_server_url(input), Err(PushError::InvalidUrl(_))),
                "{input:?} should be rejected"
            );
        }
    }

    #[test]
    fn labels_default_to_host_and_are_capped() {
        assert_eq!(default_label(None, "ci.example.com"), "ci.example.com");
        assert_eq!(
            default_label(Some("   "), "ci.example.com"),
            "ci.example.com"
        );
        assert_eq!(default_label(Some("  Prod  "), "h"), "Prod");
        assert_eq!(
            default_label(Some(&"é".repeat(100)), "h").chars().count(),
            64
        );
    }

    #[test]
    fn permission_labels() {
        assert_eq!(permission_label(PermissionState::Granted), "granted");
        assert_eq!(permission_label(PermissionState::Denied), "denied");
        assert_eq!(permission_label(PermissionState::Prompt), "prompt");
        assert_eq!(
            permission_label(PermissionState::PromptWithRationale),
            "prompt"
        );
    }

    #[test]
    fn maps_status_codes() {
        let server = Peer::Server("ci.example.com");
        assert!(matches!(
            status_error(&server, 401, None),
            PushError::Unauthorized
        ));
        assert!(matches!(
            status_error(&server, 403, None),
            PushError::Unauthorized
        ));
        assert!(matches!(
            status_error(&server, 422, Some("PUSH_RELAY_NOT_CONFIGURED")),
            PushError::ServerRelayMissing
        ));
        assert!(matches!(
            status_error(&server, 422, Some("OTHER")),
            PushError::Unexpected(_, 422)
        ));
        assert!(matches!(
            status_error(&server, 429, None),
            PushError::RateLimited
        ));
        assert!(matches!(
            status_error(&Peer::Relay, 429, None),
            PushError::RateLimited
        ));
        assert!(matches!(
            status_error(&Peer::Relay, 410, None),
            PushError::RelayGone
        ));
        assert!(matches!(
            status_error(&server, 404, None),
            PushError::Unexpected(_, 404)
        ));
        assert!(matches!(
            status_error(&Peer::Relay, 401, None),
            PushError::Unexpected(_, 401)
        ));
        assert_eq!(
            status_error(&server, 502, None).to_string(),
            "could not reach ci.example.com"
        );
        assert_eq!(
            status_error(&Peer::Relay, 503, None).to_string(),
            "could not reach the push relay"
        );
    }

    #[test]
    fn error_response_carries_the_code() {
        let response = ErrorResponse::from(PushError::ServerRelayMissing);
        assert_eq!(response.error, "this server has no push relay configured");
        assert_eq!(
            response.details.as_deref(),
            Some("PUSH_RELAY_NOT_CONFIGURED")
        );
    }
}
