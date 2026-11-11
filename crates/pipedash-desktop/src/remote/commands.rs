use std::time::Duration;

use tauri::{
    AppHandle,
    Manager,
    State,
};

use super::connection::RemoteConnection;
use super::model::{
    ConnectionInfo,
    RemoteError,
    RemoteResponse,
};
use super::pairing::{
    disconnect,
    pair,
    run_here,
};
use super::session::connect::CONNECT_TIMEOUT;
use super::store::SavedMode;
use crate::error::ErrorResponse;
use crate::export_file::{
    export_error,
    share_bytes,
    ExportFileResult,
};
use crate::AppDataDir;

const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 30_000;
const MIN_REQUEST_TIMEOUT_MS: u64 = 1_000;
const MAX_REQUEST_TIMEOUT_MS: u64 = 600_000;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_millis(MAX_REQUEST_TIMEOUT_MS);
const TICKET_LEN: usize = 32;

pub(super) fn validate_request(method: &str, path: &str) -> Result<(), RemoteError> {
    if matches!(method, "GET" | "POST" | "PUT" | "PATCH" | "DELETE")
        && path.starts_with('/')
        && !path.starts_with("//")
    {
        Ok(())
    } else {
        Err(RemoteError::InvalidRequest)
    }
}

pub(super) fn is_ticket(ticket: &str) -> bool {
    ticket.len() == TICKET_LEN
        && ticket
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[tauri::command]
pub fn get_connection(remote: State<'_, RemoteConnection>) -> ConnectionInfo {
    remote.info()
}

#[tauri::command]
pub async fn run_on_this_device(
    app: AppHandle, remote: State<'_, RemoteConnection>,
) -> Result<ConnectionInfo, ErrorResponse> {
    let _op = remote.op.lock().await;
    run_here(&app, &remote).await.map_err(Into::into)
}

#[tauri::command]
pub async fn pair_with_server(
    app: AppHandle, remote: State<'_, RemoteConnection>, address: String, code: Option<String>,
    allow_private_http: bool,
) -> Result<ConnectionInfo, ErrorResponse> {
    let _op = remote.op.lock().await;
    pair(&app, &remote, &address, code.as_deref(), allow_private_http)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn disconnect_server(
    app: AppHandle, remote: State<'_, RemoteConnection>,
) -> Result<ConnectionInfo, ErrorResponse> {
    let _op = remote.op.lock().await;
    disconnect(&app, &remote).await.map_err(Into::into)
}

#[tauri::command]
pub async fn reconnect_server(app: AppHandle) -> Result<(), ErrorResponse> {
    super::reconnect(&app);

    Ok(())
}

#[tauri::command]
pub async fn set_allow_private_http(
    app: AppHandle, remote: State<'_, RemoteConnection>, enabled: bool,
) -> Result<(), ErrorResponse> {
    let _op = remote.op.lock().await;
    remote
        .update_saved(&app.state::<AppDataDir>().0, |saved| {
            if !enabled
                && matches!(&saved.mode, SavedMode::Client { server_url, .. } if server_url.starts_with("http://"))
            {
                return Err(RemoteError::InsecureInUse);
            }
            saved.allow_private_http = enabled;
            Ok(true)
        })
        .await?;

    Ok(())
}

#[tauri::command]
pub async fn remote_request(
    remote: State<'_, RemoteConnection>, method: String, path: String, body: Option<String>,
    timeout_ms: Option<u64>,
) -> Result<RemoteResponse, ErrorResponse> {
    validate_request(&method, &path)?;
    let timeout = Duration::from_millis(
        timeout_ms
            .unwrap_or(DEFAULT_REQUEST_TIMEOUT_MS)
            .clamp(MIN_REQUEST_TIMEOUT_MS, MAX_REQUEST_TIMEOUT_MS),
    );
    let started = tokio::time::Instant::now();
    let session = remote.ready_session(timeout.min(CONNECT_TIMEOUT)).await?;
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
    app: AppHandle, remote: State<'_, RemoteConnection>, ticket: String, file_name: String,
    destination: Option<String>,
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
