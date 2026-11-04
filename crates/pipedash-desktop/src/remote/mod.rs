pub(crate) mod alerts;
mod session;
mod store;
#[cfg(test)]
pub(crate) mod test_host;

use std::path::Path;
use std::sync::{
    Arc,
    Mutex,
    MutexGuard,
    PoisonError,
};
use std::time::Duration;

use pipedash_core::domain::pairing::{
    normalize_pairing_code,
    parse_pairing_link,
};
use pipedash_core::domain::ServerUrl;
use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tauri::{
    AppHandle,
    Manager,
    State,
};

use self::session::{
    ClientHello,
    Session,
    WsStream,
};
use self::store::HostKind;
pub use self::store::{
    SavedConnection,
    SavedMode,
};
use crate::app_lock::AppLockState;
use crate::commands::ErrorResponse;
use crate::core_boot::BootKind;
use crate::desktop_lifecycle::OperationLease;
use crate::export_file::{
    export_error,
    share_bytes,
    ExportFileResult,
};
use crate::{
    AppDataDir,
    MaybeCoreContext,
};

const DEVICE_NAME_MAX_CHARS: usize = 64;
const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 30_000;
const MIN_REQUEST_TIMEOUT_MS: u64 = 1_000;
const MAX_REQUEST_TIMEOUT_MS: u64 = 600_000;
const LEAVE_TIMEOUT: Duration = Duration::from_secs(3);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_millis(MAX_REQUEST_TIMEOUT_MS);
const TICKET_LEN: usize = 32;

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error("Invalid server address: {0}")]
    InvalidUrl(&'static str),
    #[error("Enter the 8-character pairing code.")]
    CodeRequired,
    #[error("Pairing codes have 8 letters and numbers, like ABCD-EFGH.")]
    InvalidCode,
    #[error("Use https:// for servers outside your private network.")]
    PublicHttp,
    #[error("This server uses an unencrypted connection.")]
    PrivateHttpDisabled,
    #[error("This device connects over HTTP. Disconnect from the server before turning this off.")]
    InsecureInUse,
    #[error("{message}")]
    Rejected { code: String, message: String },
    #[error("Couldn't reach {0}. Check the address and that this device can reach it.")]
    Unreachable(String),
    #[error("The server took too long to answer.")]
    Timeout,
    #[error("This device isn't connected to a server.")]
    NotConnected,
    #[error("Disconnect from the server first.")]
    Connected,
    #[error("Invalid request.")]
    InvalidRequest,
    #[error("Couldn't access the system keychain.")]
    Keychain,
    #[error("Couldn't save the connection settings.")]
    Storage,
    #[error("Couldn't start Pipedash on this device: {0}")]
    Local(String),
    #[error("Couldn't download the file (HTTP {0}).")]
    Download(u16),
}

impl RemoteError {
    fn code(&self) -> &str {
        match self {
            Self::InvalidUrl(_) => "INVALID_URL",
            Self::CodeRequired => "CODE_REQUIRED",
            Self::InvalidCode => "INVALID_CODE",
            Self::PublicHttp => "PUBLIC_HTTP",
            Self::PrivateHttpDisabled => "PRIVATE_HTTP_DISABLED",
            Self::InsecureInUse => "INSECURE_IN_USE",
            Self::Rejected { code, .. } => code,
            Self::Unreachable(_) => "UNREACHABLE",
            Self::Timeout => "TIMEOUT",
            Self::NotConnected => "NOT_CONNECTED",
            Self::Connected => "CONNECTED",
            Self::InvalidRequest => "INVALID_REQUEST",
            Self::Keychain => "KEYCHAIN",
            Self::Storage => "STORAGE",
            Self::Local(_) => "LOCAL_START",
            Self::Download(_) => "DOWNLOAD_FAILED",
        }
    }
}

impl From<RemoteError> for ErrorResponse {
    fn from(error: RemoteError) -> Self {
        Self {
            error: error.to_string(),
            details: None,
            code: Some(error.code().to_owned()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemoteStatus {
    Connecting,
    Connected,
    Reconnecting,
    Removed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ConnectionMode {
    Unset,
    Standalone,
    Client,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    mode: ConnectionMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    server_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    host_kind: Option<HostKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<RemoteStatus>,
    allow_private_http: bool,
}

fn display_host(server_url: &str, host_name: Option<&str>) -> String {
    host_name.map(str::to_owned).unwrap_or_else(|| {
        ServerUrl::parse(server_url).map_or_else(|_| server_url.to_owned(), |parsed| parsed.host)
    })
}

impl ConnectionInfo {
    fn new(saved: &SavedConnection, status: RemoteStatus) -> Self {
        let mut info = Self {
            mode: ConnectionMode::Unset,
            server_url: None,
            host: None,
            host_kind: None,
            device_id: None,
            status: None,
            allow_private_http: saved.allow_private_http,
        };
        match &saved.mode {
            SavedMode::Unset => {}
            SavedMode::Standalone => info.mode = ConnectionMode::Standalone,
            SavedMode::Client {
                server_url,
                device_id,
                host_kind,
                host_name,
            } => {
                info.mode = ConnectionMode::Client;
                info.host = Some(display_host(server_url, host_name.as_deref()));
                info.server_url = Some(server_url.clone());
                info.host_kind = Some(*host_kind);
                info.device_id = Some(device_id.clone());
                info.status = Some(status);
            }
        }

        info
    }
}

#[derive(Debug, Serialize)]
pub struct RemoteResponse {
    status: u16,
    body: String,
}

struct Inner {
    saved: SavedConnection,
    token: Option<String>,
    status: RemoteStatus,
    attempts: u32,
    generation: u64,
    session: Option<Arc<Session>>,
    task: Option<JoinHandle<()>>,
}

struct Departed {
    session: Option<Arc<Session>>,
    task: Option<JoinHandle<()>>,
}

impl Departed {
    async fn leave(self, device_id: Option<&str>) {
        if let (Some(session), Some(device_id)) = (&self.session, device_id) {
            let path = format!("/devices/{device_id}");
            if let Err(error) = session.request("DELETE", &path, None, LEAVE_TIMEOUT).await {
                tracing::debug!(%error, "Couldn't tell the old host this device left");
            }
        }
        if let Some(task) = self.task {
            task.abort();
        }
    }
}

enum Readiness {
    Ready(Arc<Session>),
    Waiting(RemoteError),
    Failed(RemoteError),
}

pub struct RemoteConnection {
    inner: Mutex<Inner>,
    op: tokio::sync::Mutex<()>,
    changed: tokio::sync::watch::Sender<u64>,
}

impl RemoteConnection {
    pub fn new(saved: SavedConnection) -> Self {
        let client = matches!(saved.mode, SavedMode::Client { .. });
        let (token, status) = if client {
            match store::get_token() {
                Ok(Some(token)) => (Some(token), RemoteStatus::Connecting),
                Ok(None) => (None, RemoteStatus::Removed),
                Err(error) => {
                    tracing::warn!(%error, "Couldn't read the device token yet");
                    (None, RemoteStatus::Connecting)
                }
            }
        } else {
            (None, RemoteStatus::Connecting)
        };

        Self {
            inner: Mutex::new(Inner {
                saved,
                token,
                status,
                attempts: 0,
                generation: 0,
                session: None,
                task: None,
            }),
            op: tokio::sync::Mutex::new(()),
            changed: tokio::sync::watch::channel(0).0,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn is_client(&self) -> bool {
        matches!(self.lock().saved.mode, SavedMode::Client { .. })
    }

    fn saved(&self) -> SavedConnection {
        self.lock().saved.clone()
    }

    fn alerts_seen(&self) -> Option<i64> {
        self.lock().saved.alerts_seen
    }

    fn update_saved(
        &self, dir: &Path, change: impl FnOnce(&mut SavedConnection) -> Result<bool, RemoteError>,
    ) -> Result<(), RemoteError> {
        let mut inner = self.lock();
        let mut saved = inner.saved.clone();
        if change(&mut saved)? {
            store::write(dir, &saved)?;
            inner.saved = saved;
        }

        Ok(())
    }

    fn move_alerts_seen(&self, dir: &Path, seen: i64, allow_lower: bool) {
        let moved = self.update_saved(dir, |saved| {
            let current = saved.alerts_seen;
            if !matches!(saved.mode, SavedMode::Client { .. })
                || current == Some(seen)
                || (!allow_lower && current.is_some_and(|current| current > seen))
            {
                return Ok(false);
            }
            saved.alerts_seen = Some(seen);
            Ok(true)
        });
        if let Err(error) = moved {
            tracing::debug!(%error, "Couldn't save the alert position");
        }
    }

    pub(crate) fn preferences_pending(&self) -> bool {
        self.lock().saved.preferences_pending
    }

    pub(crate) fn set_preferences_pending(&self, dir: &Path, pending: bool) {
        let saved = self.update_saved(dir, |saved| {
            if !matches!(saved.mode, SavedMode::Client { .. })
                || saved.preferences_pending == pending
            {
                return Ok(false);
            }
            saved.preferences_pending = pending;
            Ok(true)
        });
        if let Err(error) = saved {
            tracing::debug!(%error, "Couldn't save the preference sync state");
        }
    }

    fn info(&self) -> ConnectionInfo {
        let inner = self.lock();
        ConnectionInfo::new(&inner.saved, inner.status)
    }

    fn can_reconnect(&self) -> bool {
        let inner = self.lock();
        matches!(inner.saved.mode, SavedMode::Client { .. })
            && inner.status != RemoteStatus::Removed
    }

    fn readiness(&self) -> Readiness {
        let inner = self.lock();
        let SavedMode::Client {
            server_url,
            host_name,
            ..
        } = &inner.saved.mode
        else {
            return Readiness::Failed(RemoteError::NotConnected);
        };
        let unreachable =
            || RemoteError::Unreachable(display_host(server_url, host_name.as_deref()));

        match (&inner.session, inner.status) {
            (Some(session), RemoteStatus::Connected) => Readiness::Ready(Arc::clone(session)),
            (_, RemoteStatus::Removed) => Readiness::Failed(unreachable()),
            _ => Readiness::Waiting(unreachable()),
        }
    }

    fn session(&self) -> Result<Arc<Session>, RemoteError> {
        match self.readiness() {
            Readiness::Ready(session) => Ok(session),
            Readiness::Waiting(error) | Readiness::Failed(error) => Err(error),
        }
    }

    async fn ready_session(&self, timeout: Duration) -> Result<Arc<Session>, RemoteError> {
        let mut changes = self.changed.subscribe();
        let waited = tokio::time::timeout(timeout, async {
            loop {
                match self.readiness() {
                    Readiness::Ready(session) => return Ok(session),
                    Readiness::Failed(error) => return Err(error),
                    Readiness::Waiting(_) => {}
                }
                if changes.changed().await.is_err() {
                    return Err(RemoteError::NotConnected);
                }
            }
        })
        .await;

        waited.unwrap_or_else(|_| self.session())
    }

    fn notify(&self) {
        self.changed.send_modify(|version| *version += 1);
    }

    fn launch(&self, app: &AppHandle, first: Option<WsStream>) {
        let mut inner = self.lock();
        let SavedMode::Client { server_url, .. } = &inner.saved.mode else {
            return;
        };
        let token = inner.token.clone();
        let parsed = ServerUrl::parse(server_url);
        let Ok(url) = parsed else {
            inner.status = RemoteStatus::Removed;
            self.notify();
            return;
        };

        if let Some(task) = inner.task.take() {
            task.abort();
        }
        inner.generation += 1;
        inner.status = RemoteStatus::Connecting;
        inner.attempts = 0;
        inner.session = None;
        self.notify();
        let generation = inner.generation;
        inner.task = Some(tauri::async_runtime::spawn(session::run(
            app.clone(),
            generation,
            url,
            token,
            first,
        )));
    }

    fn replace(&self, saved: SavedConnection, token: Option<String>) -> (SavedMode, Departed) {
        let mut inner = self.lock();
        inner.generation += 1;
        inner.status = RemoteStatus::Connecting;
        inner.attempts = 0;
        inner.token = token;
        self.notify();
        let previous = std::mem::replace(&mut inner.saved, saved);
        let departed = Departed {
            session: inner.session.take(),
            task: inner.task.take(),
        };

        (previous.mode, departed)
    }

    fn attach(&self, generation: u64, session: Arc<Session>) -> bool {
        let mut inner = self.lock();
        if inner.generation != generation {
            return false;
        }
        inner.session = Some(session);
        inner.status = RemoteStatus::Connected;
        inner.attempts = 0;
        self.notify();

        true
    }

    fn lost(&self, generation: u64) -> Option<u32> {
        let mut inner = self.lock();
        if inner.generation != generation {
            return None;
        }
        inner.session = None;
        inner.status = RemoteStatus::Reconnecting;
        inner.attempts += 1;
        self.notify();

        Some(inner.attempts)
    }

    fn remember_token(&self, generation: u64, token: &str) {
        let mut inner = self.lock();
        if inner.generation == generation {
            inner.token = Some(token.to_owned());
        }
    }

    fn mark_removed(&self, generation: u64) -> bool {
        let mut inner = self.lock();
        if inner.generation != generation {
            return false;
        }
        inner.session = None;
        inner.task = None;
        inner.status = RemoteStatus::Removed;
        self.notify();

        true
    }
}

pub fn startup_mode(dir: &Path, config_path: &Path) -> SavedConnection {
    store::read(dir).unwrap_or_else(|| SavedConnection {
        mode: if config_path.exists()
            || dir.join("pipedash.db").exists()
            || dir.join("storage_config.json").exists()
        {
            SavedMode::Standalone
        } else {
            SavedMode::Unset
        },
        allow_private_http: false,
        alerts_seen: None,
        preferences_pending: false,
    })
}

pub fn start(app: &AppHandle) {
    reconnect(app);
}

fn reconnect(app: &AppHandle) {
    let remote = app.state::<RemoteConnection>();
    if remote.can_reconnect() {
        remote.launch(app, None);
    }
}

#[cfg(mobile)]
pub fn handle_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if matches!(event, tauri::WindowEvent::Resumed) {
        reconnect(window.app_handle());
    }
}

pub(crate) fn check_transport(
    url: &ServerUrl, allow_private_http: bool,
) -> Result<(), RemoteError> {
    if url.scheme == "https" {
        return Ok(());
    }
    if !url.is_private() {
        return Err(RemoteError::PublicHttp);
    }
    if !allow_private_http {
        return Err(RemoteError::PrivateHttpDisabled);
    }

    Ok(())
}

fn platform_label(platform: &str) -> &'static str {
    match platform {
        "macos" => "Mac",
        "windows" => "Windows PC",
        "ios" => "iPhone or iPad",
        "android" => "Android device",
        _ => "Linux PC",
    }
}

fn name_for(hostname: &str, platform: &str) -> String {
    let hostname = hostname.trim();
    let name: String = hostname
        .strip_suffix(".local")
        .unwrap_or(hostname)
        .chars()
        .take(DEVICE_NAME_MAX_CHARS)
        .collect();
    if name.is_empty() || name.eq_ignore_ascii_case("localhost") {
        platform_label(platform).to_owned()
    } else {
        name
    }
}

pub fn device_name() -> String {
    name_for(&tauri_plugin_os::hostname(), tauri_plugin_os::platform())
}

fn validate_request(method: &str, path: &str) -> Result<(), RemoteError> {
    if matches!(method, "GET" | "POST" | "PUT" | "PATCH" | "DELETE")
        && path.starts_with('/')
        && !path.starts_with("//")
    {
        Ok(())
    } else {
        Err(RemoteError::InvalidRequest)
    }
}

fn is_ticket(ticket: &str) -> bool {
    ticket.len() == TICKET_LEN
        && ticket
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

async fn stop_local_core(app: &AppHandle) {
    let lock = app.state::<AppLockState>();
    let _transition = lock.transition().await;

    crate::host::stop(app).await;
    crate::build_watch::interrupt(app);
    if let Err(error) = crate::local_notifications::reset(app).await {
        tracing::warn!(?error, "Couldn't clear the local notifications");
    }
    let core = app.state::<MaybeCoreContext>().0.write().await.take();
    if let Some(core) = core {
        core.shutdown().await;
    }
}

#[tauri::command]
pub fn get_connection(remote: State<'_, RemoteConnection>) -> ConnectionInfo {
    remote.info()
}

#[tauri::command]
pub async fn run_on_this_device(
    _operation: OperationLease, app: AppHandle, remote: State<'_, RemoteConnection>,
) -> Result<ConnectionInfo, ErrorResponse> {
    let _op = remote.op.lock().await;
    run_here(&app, &remote).await.map_err(Into::into)
}

async fn run_here(
    app: &AppHandle, remote: &RemoteConnection,
) -> Result<ConnectionInfo, RemoteError> {
    match remote.saved().mode {
        SavedMode::Standalone => return Ok(remote.info()),
        SavedMode::Client { .. } => return Err(RemoteError::Connected),
        SavedMode::Unset => {}
    }
    alerts::exit_cleanup(app).await;

    let dir = &app.state::<AppDataDir>().0;
    remote.update_saved(dir, |saved| {
        saved.mode = SavedMode::Standalone;
        Ok(true)
    })?;

    crate::ensure_desktop_config(dir, &dir.join("config.toml")).map_err(RemoteError::Local)?;
    if !app.state::<AppLockState>().locked() {
        crate::core_boot::boot_core(app, BootKind::Launch, false)
            .await
            .map_err(|e| RemoteError::Local(e.to_string()))?;
    }
    crate::host::start_if_enabled(app).await;

    Ok(remote.info())
}

#[tauri::command]
pub async fn pair_with_server(
    _operation: OperationLease, app: AppHandle, remote: State<'_, RemoteConnection>,
    address: String, code: Option<String>, allow_private_http: bool,
) -> Result<ConnectionInfo, ErrorResponse> {
    let _op = remote.op.lock().await;
    pair(&app, &remote, &address, code.as_deref(), allow_private_http)
        .await
        .map_err(Into::into)
}

fn restore_token(previous: Option<&str>) {
    let restored = match previous {
        Some(token) => store::set_token(token),
        None => store::delete_token(),
    };
    if let Err(error) = restored {
        tracing::warn!(%error, "Couldn't restore the previous device token");
    }
}

pub(crate) fn parse_pairing(
    address: &str, code: Option<&str>,
) -> Result<(ServerUrl, String), RemoteError> {
    let link = parse_pairing_link(address);
    let (address, code) = match &link {
        Some((url, code)) => (url.as_str(), code.as_str()),
        None => (
            address,
            code.map(str::trim)
                .filter(|code| !code.is_empty())
                .ok_or(RemoteError::CodeRequired)?,
        ),
    };
    let url = ServerUrl::parse(address).map_err(RemoteError::InvalidUrl)?;
    let code = normalize_pairing_code(code).ok_or(RemoteError::InvalidCode)?;

    Ok((url, code))
}

async fn pair(
    app: &AppHandle, remote: &RemoteConnection, address: &str, code: Option<&str>,
    allow_private_http: bool,
) -> Result<ConnectionInfo, RemoteError> {
    let (url, code) = parse_pairing(address, code)?;
    let allow = allow_private_http || remote.saved().allow_private_http;
    check_transport(&url, allow)?;
    let name = device_name();
    let hello = ClientHello::Claim {
        code: &code,
        name: &name,
        platform: tauri_plugin_os::platform(),
    };
    let (stream, ok) = session::open(&url, hello).await?;
    let (Some(token), Some(device_id)) = (ok.token, ok.device_id) else {
        return Err(RemoteError::Unreachable(url.host));
    };

    let previous_token = remote
        .lock()
        .token
        .clone()
        .or_else(|| store::get_token().ok().flatten());
    if let Err(error) = store::set_token(&token) {
        session::revoke(stream, &device_id).await;
        return Err(error);
    }
    let next = SavedConnection {
        mode: SavedMode::Client {
            server_url: url.url,
            device_id: device_id.clone(),
            host_kind: ok.host,
            host_name: ok.name,
        },
        allow_private_http: allow,
        alerts_seen: None,
        preferences_pending: false,
    };
    if let Err(error) = store::write(&app.state::<AppDataDir>().0, &next) {
        restore_token(previous_token.as_deref());
        session::revoke(stream, &device_id).await;
        return Err(error);
    }

    let (previous, departed) = remote.replace(next, Some(token));
    let old_device = match &previous {
        SavedMode::Client { device_id, .. } => Some(device_id.as_str()),
        _ => None,
    };
    departed.leave(old_device).await;
    if previous == SavedMode::Standalone {
        stop_local_core(app).await;
    }
    remote.launch(app, Some(stream));

    Ok(remote.info())
}

#[tauri::command]
pub async fn disconnect_server(
    _operation: OperationLease, app: AppHandle, remote: State<'_, RemoteConnection>,
) -> Result<ConnectionInfo, ErrorResponse> {
    let _op = remote.op.lock().await;
    disconnect(&app, &remote).await.map_err(Into::into)
}

async fn disconnect(
    app: &AppHandle, remote: &RemoteConnection,
) -> Result<ConnectionInfo, RemoteError> {
    let saved = remote.saved();
    let SavedMode::Client { device_id, .. } = &saved.mode else {
        return Ok(remote.info());
    };

    let next = SavedConnection {
        mode: SavedMode::Unset,
        allow_private_http: saved.allow_private_http,
        alerts_seen: None,
        preferences_pending: false,
    };
    store::write(&app.state::<AppDataDir>().0, &next)?;
    let (_, departed) = remote.replace(next, None);
    if let Err(error) = store::delete_token() {
        tracing::warn!(%error, "Couldn't remove the device token");
    }
    departed.leave(Some(device_id)).await;
    alerts::exit_cleanup(app).await;

    Ok(remote.info())
}

#[tauri::command]
pub async fn reconnect_server(
    _operation: OperationLease, app: AppHandle,
) -> Result<(), ErrorResponse> {
    reconnect(&app);

    Ok(())
}

#[tauri::command]
pub async fn set_allow_private_http(
    _operation: OperationLease, app: AppHandle, remote: State<'_, RemoteConnection>, enabled: bool,
) -> Result<(), ErrorResponse> {
    let _op = remote.op.lock().await;
    remote.update_saved(&app.state::<AppDataDir>().0, |saved| {
        if !enabled
            && matches!(&saved.mode, SavedMode::Client { server_url, .. } if server_url.starts_with("http://"))
        {
            return Err(RemoteError::InsecureInUse);
        }
        saved.allow_private_http = enabled;
        Ok(true)
    })?;

    Ok(())
}

#[tauri::command]
pub async fn remote_request(
    _operation: OperationLease, remote: State<'_, RemoteConnection>, method: String, path: String,
    body: Option<String>, timeout_ms: Option<u64>,
) -> Result<RemoteResponse, ErrorResponse> {
    validate_request(&method, &path)?;
    let timeout = Duration::from_millis(
        timeout_ms
            .unwrap_or(DEFAULT_REQUEST_TIMEOUT_MS)
            .clamp(MIN_REQUEST_TIMEOUT_MS, MAX_REQUEST_TIMEOUT_MS),
    );
    let started = tokio::time::Instant::now();
    let session = remote
        .ready_session(timeout.min(session::CONNECT_TIMEOUT))
        .await?;
    let (status, bytes) = session
        .request(
            &method,
            &path,
            body.as_deref(),
            timeout.saturating_sub(started.elapsed()),
        )
        .await?;
    let body = String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned());

    Ok(RemoteResponse { status, body })
}

#[tauri::command]
pub async fn remote_save_download(
    _operation: OperationLease, app: AppHandle, remote: State<'_, RemoteConnection>,
    ticket: String, file_name: String, destination: Option<String>,
) -> Result<ExportFileResult, ErrorResponse> {
    if !is_ticket(&ticket) {
        return Err(RemoteError::InvalidRequest.into());
    }
    let destination = if cfg!(mobile) {
        None
    } else {
        Some(
            destination
                .filter(|path| !path.is_empty())
                .ok_or_else(|| export_error("Choose where to save the log".to_string()))?,
        )
    };

    let (status, bytes) = remote
        .session()?
        .request(
            "GET",
            &format!("/downloads/{ticket}"),
            None,
            DOWNLOAD_TIMEOUT,
        )
        .await?;
    if status != 200 {
        return Err(RemoteError::Download(status).into());
    }

    match destination {
        Some(path) => {
            tokio::fs::write(&path, &bytes)
                .await
                .map_err(|e| export_error(format!("Couldn't write the log: {e}")))?;
            Ok(ExportFileResult { path, shared: true })
        }
        None => share_bytes(&app, &file_name, &bytes).await,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn url(input: &str) -> ServerUrl {
        ServerUrl::parse(input).unwrap()
    }

    fn client(server_url: &str, host_name: Option<&str>) -> SavedConnection {
        SavedConnection {
            mode: SavedMode::Client {
                server_url: server_url.to_string(),
                device_id: "d1".to_string(),
                host_kind: HostKind::Desktop,
                host_name: host_name.map(str::to_string),
            },
            allow_private_http: true,
            alerts_seen: None,
            preferences_pending: false,
        }
    }

    #[test]
    fn a_saved_connection_wins_over_the_files_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        std::fs::write(&config, "").unwrap();
        let saved = client("https://ci.example.com", None);
        store::write(dir.path(), &saved).unwrap();

        assert_eq!(startup_mode(dir.path(), &config), saved);
    }

    #[test]
    fn unsent_notification_preferences_stay_pending_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("config.toml");
        let remote = RemoteConnection::new(SavedConnection::default());
        remote.lock().saved = client("https://ci.example.com", None);

        remote.set_preferences_pending(dir.path(), true);
        let restarted = startup_mode(dir.path(), &config);
        assert!(restarted.preferences_pending);

        remote.set_preferences_pending(dir.path(), false);
        assert!(!startup_mode(dir.path(), &config).preferences_pending);

        let standalone = RemoteConnection::new(SavedConnection::default());
        standalone.set_preferences_pending(dir.path(), true);
        assert!(!standalone.preferences_pending());
    }

    #[test]
    fn an_existing_install_without_a_saved_connection_stays_standalone() {
        for file in ["config.toml", "pipedash.db", "storage_config.json"] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::write(dir.path().join(file), "").unwrap();

            let saved = startup_mode(dir.path(), &dir.path().join("config.toml"));

            assert_eq!(saved.mode, SavedMode::Standalone, "{file}");
            assert!(!dir.path().join("connection.json").exists());
        }
    }

    #[test]
    fn an_empty_data_directory_waits_for_onboarding_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();

        let saved = startup_mode(dir.path(), &dir.path().join("config.toml"));

        assert_eq!(saved, SavedConnection::default());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn plain_http_needs_a_private_address_and_the_setting() {
        assert!(check_transport(&url("https://ci.example.com"), false).is_ok());
        assert!(check_transport(&url("https://8.8.8.8"), false).is_ok());
        assert!(check_transport(&url("http://192.168.1.20:8080"), true).is_ok());
        assert!(check_transport(&url("http://mac.local:8080"), true).is_ok());
        assert!(matches!(
            check_transport(&url("http://192.168.1.20:8080"), false),
            Err(RemoteError::PrivateHttpDisabled)
        ));
        assert!(matches!(
            check_transport(&url("http://example.com"), true),
            Err(RemoteError::PublicHttp)
        ));
        assert!(matches!(
            check_transport(&url("http://8.8.8.8"), false),
            Err(RemoteError::PublicHttp)
        ));
    }

    #[test]
    fn requests_need_a_known_method_and_an_absolute_path() {
        for (method, path) in [
            ("GET", "/providers"),
            ("POST", "/providers/1/refresh"),
            ("PUT", "/x"),
            ("PATCH", "/x"),
            ("DELETE", "/devices/d1"),
        ] {
            assert!(validate_request(method, path).is_ok(), "{method} {path}");
        }
        for (method, path) in [
            ("GET", "//evil.example.com/x"),
            ("GET", "//x"),
            ("GET", "x"),
            ("GET", ""),
            ("get", "/x"),
            ("CONNECT", "/x"),
            ("TRACE", "/x"),
        ] {
            assert!(
                matches!(
                    validate_request(method, path),
                    Err(RemoteError::InvalidRequest)
                ),
                "{method} {path}"
            );
        }
    }

    #[test]
    fn download_tickets_are_32_lowercase_hex_characters() {
        assert!(is_ticket(&"a1".repeat(16)));
        for ticket in [
            "",
            "abc",
            &"A1".repeat(16),
            &"g1".repeat(16),
            &"a1".repeat(17),
        ] {
            assert!(!is_ticket(ticket), "{ticket}");
        }
    }

    #[test]
    fn the_device_name_is_the_host_name_or_a_platform_label() {
        assert_eq!(
            name_for("  Henrique's MacBook  ", "macos"),
            "Henrique's MacBook"
        );
        assert_eq!(
            name_for("Henriques-MacBook-Pro.local", "macos"),
            "Henriques-MacBook-Pro"
        );
        assert_eq!(name_for("", "macos"), "Mac");
        assert_eq!(name_for("localhost", "ios"), "iPhone or iPad");
        assert_eq!(name_for("LocalHost", "android"), "Android device");
        assert_eq!(name_for(" ", "windows"), "Windows PC");
        assert_eq!(name_for("", "linux"), "Linux PC");
        assert_eq!(name_for(&"é".repeat(100), "linux").chars().count(), 64);
    }

    #[test]
    fn the_frontend_reads_the_connection_as_camel_case() {
        let remote = RemoteConnection::new(SavedConnection::default());
        assert_eq!(
            serde_json::to_value(remote.info()).unwrap(),
            json!({ "mode": "unset", "allowPrivateHttp": false })
        );

        let info = ConnectionInfo::new(
            &client("http://192.168.1.20:8080", Some("Studio")),
            RemoteStatus::Reconnecting,
        );
        assert_eq!(
            serde_json::to_value(info).unwrap(),
            json!({
                "mode": "client",
                "serverUrl": "http://192.168.1.20:8080",
                "host": "Studio",
                "hostKind": "desktop",
                "deviceId": "d1",
                "status": "reconnecting",
                "allowPrivateHttp": true,
            })
        );
        assert_eq!(
            ConnectionInfo::new(
                &client("https://ci.example.com:8443", None),
                RemoteStatus::Removed
            )
            .host
            .as_deref(),
            Some("ci.example.com")
        );
    }

    fn waiting_remote() -> RemoteConnection {
        let remote = RemoteConnection::new(SavedConnection::default());
        {
            let mut inner = remote.lock();
            inner.saved = client("https://ci.example.com", Some("Studio"));
            inner.status = RemoteStatus::Connecting;
        }
        remote
    }

    #[test]
    fn only_the_current_session_can_change_the_status() {
        let remote = RemoteConnection::new(SavedConnection::default());
        let (outgoing, _queue) = tokio::sync::mpsc::channel(1);
        let session = Arc::new(Session::new("host", outgoing));
        remote.lock().generation = 2;

        assert!(!remote.attach(1, Arc::clone(&session)));
        assert_eq!(remote.lost(1), None);
        assert!(!remote.mark_removed(1));
        assert_eq!(remote.lock().status, RemoteStatus::Connecting);

        assert!(remote.attach(2, session));
        assert_eq!(remote.lock().status, RemoteStatus::Connected);
        assert_eq!(remote.lost(2), Some(1));
        assert_eq!(remote.lost(2), Some(2));
        assert_eq!(remote.lock().status, RemoteStatus::Reconnecting);
        assert!(remote.lock().session.is_none());
        assert!(remote.mark_removed(2));
        assert_eq!(remote.lock().status, RemoteStatus::Removed);
    }

    #[tokio::test]
    async fn a_request_issued_while_connecting_waits_for_the_session() {
        let remote = Arc::new(waiting_remote());
        let (outgoing, _queue) = tokio::sync::mpsc::channel(1);

        let asking = Arc::clone(&remote);
        let ready = tokio::spawn(async move { asking.ready_session(Duration::from_secs(5)).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!ready.is_finished());

        assert!(remote.attach(0, Arc::new(Session::new("host", outgoing))));

        assert!(ready.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn a_request_gives_up_as_unreachable_when_nothing_connects() {
        let remote = waiting_remote();
        for status in [RemoteStatus::Connecting, RemoteStatus::Reconnecting] {
            remote.lock().status = status;

            let Err(error) = remote.ready_session(Duration::from_millis(50)).await else {
                panic!("nothing connected");
            };

            assert!(matches!(error, RemoteError::Unreachable(host) if host == "Studio"));
        }
    }

    #[tokio::test]
    async fn a_removed_device_and_a_standalone_app_fail_without_waiting() {
        let remote = waiting_remote();
        assert!(remote.mark_removed(0));
        let Err(removed) = remote.ready_session(Duration::from_secs(30)).await else {
            panic!("a removed device has no session");
        };
        assert!(matches!(removed, RemoteError::Unreachable(_)));

        let standalone = RemoteConnection::new(SavedConnection::default());
        let Err(error) = standalone.ready_session(Duration::from_secs(30)).await else {
            panic!("standalone has no server");
        };
        assert!(matches!(error, RemoteError::NotConnected));
    }

    #[test]
    fn errors_carry_a_code_the_frontend_can_branch_on() {
        let response = ErrorResponse::from(RemoteError::Unreachable("mac.local".to_string()));
        assert_eq!(response.code.as_deref(), Some("UNREACHABLE"));
        assert_eq!(
            response.error,
            "Couldn't reach mac.local. Check the address and that this device can reach it."
        );

        let response = ErrorResponse::from(RemoteError::Rejected {
            code: "PAIRING_CODE_INVALID".to_string(),
            message: "This code expired or was already used. Create a new code on the server."
                .to_string(),
        });
        assert_eq!(response.code.as_deref(), Some("PAIRING_CODE_INVALID"));
        assert!(response.error.starts_with("This code expired"));
    }
}
