mod cursor;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use pipedash_core::domain::{
    RunFinish,
    ServerUrl,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use tauri::{
    AppHandle,
    Manager,
    State,
};

use super::connection::RemoteConnection;
use super::model::{
    display_host,
    RemoteError,
};
use super::pairing::{
    check_transport,
    device_name,
};
use super::session::connect::{
    open,
    request_once,
    revoke,
    CONNECT_TIMEOUT,
};
use super::session::protocol::ClientHello;
use super::session::Session;
use super::store::{
    self,
    SavedMode,
};
use crate::error::ErrorResponse;
use crate::local_notifications::store::AlertHost;
use crate::AppDataDir;

pub const ALERT_RECORDED_EVENT: &str = "alert-recorded";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const CATCH_UP_LIMIT: u32 = 100;
const CATCH_UP_MAX_PAGES: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostAlert {
    pub id: String,
    pub seq: i64,
    pub key: String,
    pub kind: String,
    pub tone: String,
    pub title: String,
    pub body: String,
    pub pipeline_id: Option<String>,
    pub provider_id: Option<i64>,
    #[serde(default)]
    pub provider_ids: Vec<i64>,
    #[serde(default)]
    pub finish: Option<RunFinish>,
    pub created_at: String,
}

impl HostAlert {
    pub(crate) fn affected_providers(&self) -> Vec<i64> {
        if self.provider_ids.is_empty() {
            self.provider_id.into_iter().collect()
        } else {
            self.provider_ids.clone()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AlertPage {
    pub alerts: Vec<HostAlert>,
    pub latest_seq: i64,
}

fn bad_response() -> RemoteError {
    RemoteError::Rejected {
        code: "BAD_RESPONSE".to_owned(),
        message: "The host sent alerts Pipedash can't read.".to_owned(),
    }
}

fn open_target(host: &AlertHost) -> Result<ServerUrl, RemoteError> {
    let url = ServerUrl::parse(&host.server_url).map_err(RemoteError::InvalidUrl)?;
    check_transport(&url, host.allow_private_http)?;

    Ok(url)
}

pub async fn host_request(
    host: &AlertHost, method: &str, path: &str, body: Option<&str>,
) -> Result<(u16, String), RemoteError> {
    let url = open_target(host)?;
    let (mut stream, _) = open(&url, ClientHello::Auth { token: &host.token }).await?;
    let mut answer = None;
    let _ = tokio::time::timeout(REQUEST_TIMEOUT, async {
        answer = Some(request_once(&mut stream, method, path, body).await);
        let _ = stream.close(None).await;
    })
    .await;

    match answer {
        Some(Some((status, body))) => Ok((status, String::from_utf8_lossy(&body).into_owned())),
        Some(None) => Err(RemoteError::Unreachable(host.host_name.clone())),
        None => Err(RemoteError::Timeout),
    }
}

pub(crate) async fn client_request(
    remote: &RemoteConnection, method: &str, path: &str, body: Option<&str>,
) -> Result<(u16, String), RemoteError> {
    client_request_within(remote, method, path, body, REQUEST_TIMEOUT).await
}

pub(crate) async fn client_request_within(
    remote: &RemoteConnection, method: &str, path: &str, body: Option<&str>, timeout: Duration,
) -> Result<(u16, String), RemoteError> {
    let session = remote.ready_session(CONNECT_TIMEOUT).await?;
    let (status, body) = session.request(method, path, body, timeout).await?;

    Ok((status, String::from_utf8_lossy(&body).into_owned()))
}

pub async fn fetch(host: &AlertHost, after: i64, limit: u32) -> Result<AlertPage, RemoteError> {
    let path = format!("/alerts?after={after}&limit={limit}");
    let (status, body) = host_request(host, "GET", &path, None).await?;
    if status != 200 {
        return Err(RemoteError::Unreachable(host.host_name.clone()));
    }

    serde_json::from_str(&body).map_err(|_| bad_response())
}

pub async fn claim_alert_host(
    url: ServerUrl, code: &str, allow_private_http: bool,
) -> Result<AlertHost, RemoteError> {
    check_transport(&url, allow_private_http)?;
    let name = device_name();
    let hello = ClientHello::Claim {
        code,
        name: &name,
        platform: tauri_plugin_os::platform(),
    };
    let (mut stream, ok) = open(&url, hello).await?;
    let (Some(token), Some(device_id)) = (ok.token, ok.device_id) else {
        return Err(RemoteError::Unreachable(url.host));
    };

    let mut latest = None;
    let _ = tokio::time::timeout(REQUEST_TIMEOUT, async {
        latest = request_once(&mut stream, "GET", "/alerts?after=0&limit=1", None)
            .await
            .filter(|(status, _)| *status == 200)
            .and_then(|(_, body)| serde_json::from_slice::<AlertPage>(&body).ok());
        if latest.is_some() {
            let _ = stream.close(None).await;
        }
    })
    .await;
    let Some(latest) = latest else {
        revoke(stream, &device_id).await;
        return Err(RemoteError::Unreachable(url.host));
    };

    Ok(AlertHost {
        host_name: ok.name.unwrap_or_else(|| url.host.clone()),
        server_url: url.url,
        token,
        allow_private_http,
        device_id,
        standalone: true,
        notified_seq: latest.latest_seq,
        push: false,
    })
}

pub async fn revoke_device(host: &AlertHost) {
    let Ok(url) = open_target(host) else {
        return;
    };
    match open(&url, ClientHello::Auth { token: &host.token }).await {
        Ok((stream, _)) => revoke(stream, &host.device_id).await,
        Err(error) => tracing::debug!(%error, "Couldn't tell the alert host this device left"),
    }
}

pub(crate) async fn client_alert_host(remote: &RemoteConnection) -> Result<AlertHost, RemoteError> {
    let saved = remote.saved();
    let SavedMode::Client {
        server_url,
        device_id,
        host_name,
        ..
    } = saved.mode
    else {
        return Err(RemoteError::NotConnected);
    };
    let token = tauri::async_runtime::spawn_blocking(store::get_token)
        .await
        .map_err(|_| RemoteError::Keychain)??
        .ok_or(RemoteError::NotConnected)?;
    let mut host = AlertHost {
        host_name: display_host(&server_url, host_name.as_deref()),
        server_url,
        token,
        allow_private_http: saved.allow_private_http,
        device_id,
        standalone: false,
        notified_seq: 0,
        push: false,
    };
    host.notified_seq = fetch(&host, 0, 1).await?.latest_seq;

    Ok(host)
}

pub(super) struct CatchUp {
    pub alerts: Vec<HostAlert>,
    pub seen: i64,
    pub lower: bool,
    pub complete: bool,
}

async fn read_page(session: &Session, after: i64, limit: u32) -> Result<AlertPage, RemoteError> {
    let path = format!("/alerts?after={after}&limit={limit}");
    let (status, body) = session.request("GET", &path, None, REQUEST_TIMEOUT).await?;

    Some(status)
        .filter(|status| *status == 200)
        .and_then(|_| serde_json::from_slice(&body).ok())
        .ok_or_else(bad_response)
}

async fn plan_catch_up(
    session: &Session, seen: Option<i64>, limit: u32, max_pages: usize,
) -> Result<CatchUp, RemoteError> {
    let mut after = seen.unwrap_or(0);
    let mut alerts = Vec::new();
    for page_number in 0..max_pages {
        let page = read_page(session, after, limit).await?;
        if page_number == 0 && (seen.is_none() || after > page.latest_seq) {
            return Ok(CatchUp {
                alerts,
                seen: page.latest_seq,
                lower: after > page.latest_seq,
                complete: true,
            });
        }
        let full = page.alerts.len() >= limit as usize;
        if let Some(last) = page.alerts.last() {
            after = last.seq;
        } else {
            after = after.max(page.latest_seq);
        }
        alerts.extend(page.alerts);
        if !full {
            return Ok(CatchUp {
                alerts,
                seen: after,
                lower: false,
                complete: true,
            });
        }
    }

    Ok(CatchUp {
        alerts,
        seen: after,
        lower: false,
        complete: false,
    })
}

async fn take(
    session: &Session, remote: &RemoteConnection, dir: &Path, record: impl FnOnce(&[HostAlert]),
) -> Result<CatchUp, RemoteError> {
    take_pages(
        session,
        remote,
        dir,
        CATCH_UP_LIMIT,
        CATCH_UP_MAX_PAGES,
        record,
    )
    .await
}

async fn take_pages(
    session: &Session, remote: &RemoteConnection, dir: &Path, limit: u32, max_pages: usize,
    record: impl FnOnce(&[HostAlert]),
) -> Result<CatchUp, RemoteError> {
    let _gate = session.alerts_gate().await;
    let Some((generation, seen)) = remote.alerts_position(session) else {
        return Err(RemoteError::NotConnected);
    };
    let plan = plan_catch_up(session, seen, limit, max_pages).await?;
    let _op = remote.op.lock().await;
    if remote.alerts_position(session).map(|(current, _)| current) != Some(generation) {
        return Err(RemoteError::NotConnected);
    }
    record(&plan.alerts);
    if plan.lower {
        session.reset_cursor();
    }
    remote.move_alerts_seen(dir, plan.seen, plan.lower).await;
    if plan.complete {
        session.mark_alerts_synced();
    }

    Ok(plan)
}

#[tauri::command]
pub async fn take_missed_alerts<R: tauri::Runtime>(
    app: AppHandle<R>, remote: State<'_, RemoteConnection>,
) -> Result<Vec<HostAlert>, ErrorResponse> {
    if !remote.is_client() {
        return Ok(Vec::new());
    }
    let session = remote.ready_session(CONNECT_TIMEOUT).await?;
    let plan = take(&session, &remote, &app.state::<AppDataDir>().0, |_alerts| {
        #[cfg(desktop)]
        crate::desktop::notifications::record_missed(&app, _alerts);
    })
    .await?;
    if plan.lower {
        crate::local_notifications::settings::reset_alert_host_seq(plan.seen).await;
    } else {
        crate::local_notifications::settings::advance_alert_host(plan.seen).await;
    }

    Ok(plan
        .alerts
        .into_iter()
        .map(|alert| HostAlert {
            provider_ids: alert.affected_providers(),
            ..alert
        })
        .collect())
}

pub(super) fn record_live(app: &AppHandle, session: &Arc<Session>, payload: &Value) {
    let Some(seq) = payload.get("seq").and_then(Value::as_i64) else {
        return;
    };
    if !session.queue_cursor(seq) {
        return;
    }
    let app = app.clone();
    let session = Arc::clone(session);
    tauri::async_runtime::spawn(async move {
        cursor::drain(
            &app.state::<RemoteConnection>(),
            &app.state::<AppDataDir>().0,
            &session,
        )
        .await;
    });
}

pub(super) async fn exit_cleanup(app: &AppHandle) {
    crate::push::forget().await;
    if let Err(error) = crate::local_notifications::settings::reset(app).await {
        tracing::warn!(?error, "Couldn't clear the alerts from the old host");
    }
}

pub(super) fn spawn_exit_cleanup(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { exit_cleanup(&app).await });
}

#[cfg(test)]
mod tests;
