use std::sync::Arc;
use std::time::Duration;

use futures::{
    SinkExt,
    StreamExt,
};
use pipedash_core::domain::ServerUrl;
use serde::Serialize;
use serde_json::json;
use tauri::{
    AppHandle,
    Emitter,
    Manager,
};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use super::connect::{
    open,
    WsStream,
};
use super::protocol::{
    parse_server_frame,
    revokes_access,
    ClientHello,
    ServerFrame,
};
use super::Session;
use crate::remote::alerts;
use crate::remote::connection::RemoteConnection;
use crate::remote::model::{
    RemoteError,
    RemoteStatus,
};
#[cfg(desktop)]
use crate::tauri_event_bus::NOTIFICATION_EVENT;

const READ_TIMEOUT: Duration = Duration::from_secs(45);
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
const OUTBOX_CAPACITY: usize = 64;
const BACKOFF_BASE_MS: f64 = 1000.0;
const BACKOFF_MAX_MS: f64 = 30_000.0;

const CONNECTION_STATUS_EVENT: &str = "connection-status";
const REMOTE_ACCESS_REVOKED_EVENT: &str = "remote-access-revoked";

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Lost,
    Removed,
    Superseded,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusEvent {
    status: RemoteStatus,
    reconnect_attempts: u32,
}

fn emit_status(app: &AppHandle, status: RemoteStatus, reconnect_attempts: u32) {
    let event = StatusEvent {
        status,
        reconnect_attempts,
    };
    if let Err(error) = app.emit(CONNECTION_STATUS_EVENT, event) {
        tracing::warn!(%error, "Couldn't announce the connection status");
    }
}

fn remove(app: &AppHandle, generation: u64) {
    if app.state::<RemoteConnection>().mark_removed(generation) {
        if let Err(error) = app.emit(REMOTE_ACCESS_REVOKED_EVENT, json!({})) {
            tracing::warn!(%error, "Couldn't announce the removed device");
        }
        alerts::spawn_exit_cleanup(app);
    }
}

fn handle_frame(app: &AppHandle, session: &Arc<Session>, text: &str) -> Option<Outcome> {
    match parse_server_frame(text)? {
        ServerFrame::Response { id, status, body } => session.resolve(id, status, body),
        ServerFrame::AuthError { code, .. } if revokes_access(&code) => {
            return Some(Outcome::Removed)
        }
        ServerFrame::AuthError { .. } => return Some(Outcome::Lost),
        ServerFrame::AuthOk(_) => {}
        ServerFrame::Event { name, payload } => {
            if name == alerts::ALERT_RECORDED_EVENT {
                alerts::record_live(app, session, &payload);
            }
            #[cfg(desktop)]
            if name == NOTIFICATION_EVENT {
                crate::desktop::notifications::record_received(app, &payload);
            }
            #[cfg(desktop)]
            crate::desktop::projection::observe_server_event(app, &name);
            if let Err(error) = app.emit(&name, payload) {
                tracing::debug!(%error, event = %name, "Couldn't forward a server event");
            }
        }
    }

    None
}

async fn serve(app: &AppHandle, generation: u64, host: &str, stream: WsStream) -> Outcome {
    let remote = app.state::<RemoteConnection>();
    let (outgoing, mut queue) = mpsc::channel(OUTBOX_CAPACITY);
    let session = Arc::new(Session::new(host, outgoing));
    if !remote.attach(generation, Arc::clone(&session)) {
        return Outcome::Superseded;
    }
    emit_status(app, RemoteStatus::Connected, 0);
    crate::push::spawn_sync_token(app);

    let _pending = FailOnDrop(Arc::clone(&session));
    let (mut sink, mut source) = stream.split();
    let idle = tokio::time::sleep(READ_TIMEOUT);
    tokio::pin!(idle);

    loop {
        tokio::select! {
            incoming = source.next() => {
                idle.as_mut().reset(tokio::time::Instant::now() + READ_TIMEOUT);
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        if let Some(outcome) = handle_frame(app, &session, &text) {
                            break outcome;
                        }
                    }
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break Outcome::Lost,
                    Some(Ok(_)) => {}
                }
            }
            outgoing = queue.recv() => {
                let Some(message) = outgoing else { break Outcome::Lost };
                if !matches!(tokio::time::timeout(WRITE_TIMEOUT, sink.send(message)).await, Ok(Ok(()))) {
                    break Outcome::Lost;
                }
            }
            _ = &mut idle => break Outcome::Lost,
        }
    }
}

pub(super) struct FailOnDrop(pub(super) Arc<Session>);

impl Drop for FailOnDrop {
    fn drop(&mut self) {
        self.0.fail_all();
    }
}

async fn load_token() -> Result<Option<String>, RemoteError> {
    tauri::async_runtime::spawn_blocking(crate::remote::store::get_token)
        .await
        .map_err(|_| RemoteError::Keychain)?
}

pub(super) fn backoff(attempts: u32) -> Duration {
    let exponent = attempts.saturating_sub(1).min(20) as i32;
    let millis = (BACKOFF_BASE_MS * 1.5_f64.powi(exponent)).min(BACKOFF_MAX_MS);
    Duration::from_millis(millis as u64)
}

fn loses_access(error: &RemoteError) -> bool {
    matches!(error, RemoteError::Rejected { code, .. } if revokes_access(code))
}

async fn reconnect_stream(
    app: &AppHandle, generation: u64, url: &ServerUrl, token: &mut Option<String>,
) -> Result<Option<WsStream>, RemoteError> {
    if token.is_none() {
        match load_token().await {
            Ok(Some(loaded)) => {
                app.state::<RemoteConnection>()
                    .remember_token(generation, &loaded);
                *token = Some(loaded);
            }
            Ok(None) => return Err(RemoteError::NotConnected),
            Err(error) => tracing::debug!(%error, "Couldn't read the device token"),
        }
    }
    let Some(token) = token.as_deref() else {
        return Ok(None);
    };

    match open(url, ClientHello::Auth { token }).await {
        Ok((stream, _)) => Ok(Some(stream)),
        Err(error) if loses_access(&error) => Err(error),
        Err(error) => {
            tracing::debug!(%error, "Couldn't reach the server");
            Ok(None)
        }
    }
}

pub(crate) async fn run(
    app: AppHandle, generation: u64, url: ServerUrl, token: Option<String>, first: Option<WsStream>,
) {
    let mut first = first;
    let mut token = token;
    loop {
        let stream = match first.take() {
            Some(stream) => Some(stream),
            None => match reconnect_stream(&app, generation, &url, &mut token).await {
                Ok(stream) => stream,
                Err(_) => {
                    remove(&app, generation);
                    return;
                }
            },
        };

        if let Some(stream) = stream {
            match serve(&app, generation, &url.host, stream).await {
                Outcome::Superseded => return,
                Outcome::Removed => {
                    remove(&app, generation);
                    return;
                }
                Outcome::Lost => {}
            }
        }

        let Some(attempts) = app.state::<RemoteConnection>().lost(generation) else {
            return;
        };
        emit_status(&app, RemoteStatus::Reconnecting, attempts);
        tokio::time::sleep(backoff(attempts)).await;
    }
}
