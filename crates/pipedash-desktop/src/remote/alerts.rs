use std::path::Path;
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

use super::session::{
    self,
    request_once,
    ClientHello,
    Session,
};
use super::{
    check_transport,
    device_name,
    display_host,
    store,
    RemoteConnection,
    RemoteError,
    SavedMode,
};
use crate::commands::ErrorResponse;
use crate::local_notifications::{
    self,
    AlertHost,
};
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
    let (mut stream, _) = session::open(&url, ClientHello::Auth { token: &host.token }).await?;
    let answer = tokio::time::timeout(
        REQUEST_TIMEOUT,
        request_once(&mut stream, method, path, body),
    )
    .await;
    let _ = stream.close(None).await;

    match answer {
        Ok(Some((status, body))) => Ok((status, String::from_utf8_lossy(&body).into_owned())),
        Ok(None) => Err(RemoteError::Unreachable(host.host_name.clone())),
        Err(_) => Err(RemoteError::Timeout),
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
    let session = remote.ready_session(session::CONNECT_TIMEOUT).await?;
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
    let (mut stream, ok) = session::open(&url, hello).await?;
    let (Some(token), Some(device_id)) = (ok.token, ok.device_id) else {
        return Err(RemoteError::Unreachable(url.host));
    };

    let latest = tokio::time::timeout(
        REQUEST_TIMEOUT,
        request_once(&mut stream, "GET", "/alerts?after=0&limit=1", None),
    )
    .await
    .ok()
    .flatten()
    .filter(|(status, _)| *status == 200)
    .and_then(|(_, body)| serde_json::from_slice::<AlertPage>(&body).ok());
    let Some(latest) = latest else {
        session::revoke(stream, &device_id).await;
        return Err(RemoteError::Unreachable(url.host));
    };
    let _ = stream.close(None).await;

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
    match session::open(&url, ClientHello::Auth { token: &host.token }).await {
        Ok((stream, _)) => session::revoke(stream, &host.device_id).await,
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
    let (generation, seen) = {
        let inner = remote.lock();
        if !inner
            .session
            .as_ref()
            .is_some_and(|current| std::ptr::eq(current.as_ref(), session))
        {
            return Err(RemoteError::NotConnected);
        }
        (inner.generation, inner.saved.alerts_seen)
    };
    let plan = plan_catch_up(session, seen, limit, max_pages).await?;
    let _op = remote.op.lock().await;
    {
        let inner = remote.lock();
        if inner.generation != generation
            || !inner
                .session
                .as_ref()
                .is_some_and(|current| std::ptr::eq(current.as_ref(), session))
        {
            return Err(RemoteError::NotConnected);
        }
    }
    record(&plan.alerts);
    remote.move_alerts_seen(dir, plan.seen, plan.lower);
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
    let session = remote.ready_session(session::CONNECT_TIMEOUT).await?;
    let plan = take(&session, &remote, &app.state::<AppDataDir>().0, |_alerts| {
        #[cfg(desktop)]
        crate::desktop::notifications::record_missed(&app, _alerts);
    })
    .await?;
    if plan.lower {
        local_notifications::reset_alert_host_seq(plan.seen).await;
    } else {
        local_notifications::advance_alert_host(plan.seen).await;
    }

    Ok(plan.alerts)
}

fn advance_live(session: &Session, remote: &RemoteConnection, dir: &Path, seq: i64) {
    if session.alerts_synced() {
        remote.move_alerts_seen(dir, seq, false);
    }
}

pub(super) fn record_live(app: &AppHandle, session: &Session, payload: &Value) {
    let Some(seq) = payload.get("seq").and_then(Value::as_i64) else {
        return;
    };
    advance_live(
        session,
        &app.state::<RemoteConnection>(),
        &app.state::<AppDataDir>().0,
        seq,
    );
    tauri::async_runtime::spawn(local_notifications::advance_alert_host(seq));
}

pub(super) async fn exit_cleanup(app: &AppHandle) {
    crate::push::forget().await;
    if let Err(error) = local_notifications::reset(app).await {
        tracing::warn!(?error, "Couldn't clear the alerts from the old host");
    }
}

pub(super) fn spawn_exit_cleanup(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { exit_cleanup(&app).await });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pipedash_core::domain::{
        AlertKind,
        NewDeviceAlert,
    };
    use pipedash_web::pairing::create_pairing;
    use tokio::sync::mpsc;
    use tokio_tungstenite::tungstenite::Message;

    use super::*;
    use crate::remote::test_host::{
        start_host,
        Host,
    };
    use crate::remote::SavedConnection;

    async fn pair(host: &Host) -> AlertHost {
        let code = create_pairing(&host.state, &host.url.url).unwrap().code;

        claim_alert_host(host.url.clone(), &code, true)
            .await
            .unwrap()
    }

    async fn record(host: &Host, key: &str, kind: AlertKind) -> i64 {
        host.core
            .device_alerts()
            .insert(&NewDeviceAlert {
                key: key.to_owned(),
                kind,
                title: format!("{key} title"),
                body: "body".to_owned(),
                pipeline_id: None,
                provider_id: None,
                provider_ids: Vec::new(),
                finish: None,
            })
            .await
            .unwrap()
            .seq
    }

    fn bridge(host: AlertHost) -> Arc<Session> {
        let (outgoing, mut queue) = mpsc::channel(4);
        let session = Arc::new(Session::new("Studio", outgoing));
        let replying = Arc::clone(&session);
        tokio::spawn(async move {
            while let Some(Message::Text(text)) = queue.recv().await {
                let frame: Value = serde_json::from_str(&text).unwrap();
                let (status, body) = host_request(
                    &host,
                    frame["method"].as_str().unwrap(),
                    frame["path"].as_str().unwrap(),
                    None,
                )
                .await
                .unwrap();
                replying.resolve(frame["id"].as_u64().unwrap(), status, body.into_bytes());
            }
        });
        session
    }

    #[cfg(desktop)]
    #[tokio::test]
    async fn missed_alerts_reach_desktop_history_before_the_cursor_advances() {
        use tauri::Listener;

        use crate::desktop::notifications;

        let host = start_host().await;
        let alert_host = pair(&host).await;
        let first = record(&host, "pipeline:1", AlertKind::Failed).await;
        let second = record(&host, "pipeline:2", AlertKind::Recovered).await;
        let dir = tempfile::tempdir().unwrap();
        let remote = client_remote(Some(first));
        store::write(dir.path(), &remote.saved()).unwrap();
        let session = bridge(alert_host);
        assert!(remote.attach(0, session));
        let app = notifications::test_app(remote, dir.path());
        let (published, mut snapshots) = mpsc::unbounded_channel();
        let path = dir.path().to_path_buf();
        app.listen("desktop-notifications-changed", move |event| {
            let snapshot: Value = serde_json::from_str(event.payload()).unwrap();
            published
                .send((snapshot, store::read(&path).unwrap().alerts_seen))
                .unwrap();
        });

        let taken = take_missed_alerts(app.handle().clone(), app.state())
            .await
            .unwrap();
        let (published, seen_when_published) = snapshots.try_recv().unwrap();
        let history = notifications::get_desktop_notifications(app.handle().clone());

        assert_eq!(seen_when_published, Some(first));
        assert_eq!(
            app.state::<RemoteConnection>().lock().saved.alerts_seen,
            Some(second)
        );
        assert_eq!(history.items.len(), 1);
        assert_eq!(history.items[0].key, "pipeline:2");
        assert_eq!(history.items[0].tone, "success");
        assert!(!history.items[0].read);
        assert_eq!(published["items"][0]["key"], "pipeline:2");
        assert_eq!(published["items"][0]["read"], false);
        assert_eq!(
            history.items[0].at,
            chrono::DateTime::parse_from_rfc3339(&taken[0].created_at)
                .unwrap()
                .timestamp_millis()
        );
    }

    #[cfg(desktop)]
    fn missed_alert(key: &str, at: i64, kind: &str) -> HostAlert {
        HostAlert {
            id: format!("{key}-{at}"),
            seq: at,
            key: key.to_string(),
            kind: kind.to_string(),
            tone: AlertKind::parse(kind).unwrap().tone().to_string(),
            title: format!("{key} {kind}"),
            body: "body".to_string(),
            pipeline_id: Some("p".to_string()),
            provider_id: Some(1),
            provider_ids: Vec::new(),
            finish: None,
            created_at: chrono::DateTime::from_timestamp(1_700_000_000 + at, 0)
                .unwrap()
                .to_rfc3339(),
        }
    }

    #[cfg(desktop)]
    #[test]
    fn missed_alerts_keep_a_newer_live_recovery_and_its_read_state() {
        use crate::desktop::notifications;

        let dir = tempfile::tempdir().unwrap();
        let app = notifications::test_app(client_remote(Some(0)), dir.path());
        let newer = missed_alert("pipeline:p", 200, "recovered");
        let live = serde_json::json!({
            "at": newer.created_at, "key": newer.key, "kind": "recovered",
            "title": newer.title, "body": newer.body, "provider_id": 1,
            "provider_ids": [1], "pipeline_id": "p"
        });
        notifications::record_received(app.handle(), &live);
        notifications::mark_read(app.handle(), Some("pipeline:p"));

        notifications::record_missed(app.handle(), &[missed_alert("pipeline:p", 100, "failed")]);
        let history = notifications::get_desktop_notifications(app.handle().clone());
        assert_eq!(history.items[0].title, newer.title);
        assert_eq!(history.items[0].tone, "success");
        assert!(history.items[0].read);

        notifications::record_missed(app.handle(), &[newer]);
        let duplicate = notifications::get_desktop_notifications(app.handle().clone());
        assert!(duplicate.items[0].read);
        assert_eq!(duplicate.revision, history.revision);

        notifications::record_missed(app.handle(), &[missed_alert("pipeline:p", 300, "failed")]);
        let later = notifications::get_desktop_notifications(app.handle().clone());
        assert_eq!(later.items[0].tone, "danger");
        assert!(!later.items[0].read);
    }

    #[cfg(desktop)]
    #[test]
    fn older_missed_alerts_do_not_evict_the_fifty_newest_notices() {
        use crate::desktop::notifications;

        let dir = tempfile::tempdir().unwrap();
        let app = notifications::test_app(client_remote(Some(0)), dir.path());
        let newer: Vec<_> = (100..150)
            .map(|n| missed_alert(&format!("new:{n}"), n, "failed"))
            .collect();
        notifications::record_missed(app.handle(), &newer);
        let older: Vec<_> = (0..50)
            .map(|n| missed_alert(&format!("old:{n}"), n, "failed"))
            .collect();
        notifications::record_missed(app.handle(), &older);

        let history = notifications::get_desktop_notifications(app.handle().clone());
        assert_eq!(
            history
                .items
                .iter()
                .map(|notice| notice.key.clone())
                .collect::<Vec<_>>(),
            (100..150)
                .rev()
                .map(|n| format!("new:{n}"))
                .collect::<Vec<_>>()
        );
    }

    #[cfg(desktop)]
    #[tokio::test]
    async fn changing_servers_discards_an_in_flight_catch_up() {
        use crate::desktop::notifications;

        let dir = tempfile::tempdir().unwrap();
        let remote = client_remote(Some(0));
        let (outgoing, mut requests) = mpsc::channel(1);
        let session = Arc::new(Session::new("old host", outgoing));
        assert!(remote.attach(0, Arc::clone(&session)));
        let app = notifications::test_app(remote, dir.path());
        let remote = app.state::<RemoteConnection>();
        let taking = take_missed_alerts(app.handle().clone(), app.state());
        let switching = async {
            let Message::Text(request) = requests.recv().await.unwrap() else {
                panic!("expected alert request")
            };
            let request: Value = serde_json::from_str(&request).unwrap();
            {
                let _op = remote.op.lock().await;
                let mut next = remote.saved();
                next.mode = SavedMode::Client {
                    server_url: "http://127.0.0.1:2".to_string(),
                    device_id: "d2".to_string(),
                    host_kind: store::HostKind::Desktop,
                    host_name: None,
                };
                next.alerts_seen = None;
                remote.replace(next, None);
            }
            let response = serde_json::json!({"alerts": [missed_alert("old:p", 1, "failed")], "latest_seq": 1});
            session.resolve(
                request["id"].as_u64().unwrap(),
                200,
                serde_json::to_vec(&response).unwrap(),
            );
        };
        let (taken, ()) = tokio::join!(taking, switching);

        assert!(taken.is_err());
        assert_eq!(remote.lock().saved.alerts_seen, None);
        assert!(
            notifications::get_desktop_notifications(app.handle().clone())
                .items
                .is_empty()
        );
    }

    #[tokio::test]
    async fn fetch_returns_the_alerts_after_a_cursor() {
        let host = start_host().await;
        let alert_host = pair(&host).await;
        let first = record(&host, "pipeline:1", AlertKind::Failed).await;
        record(&host, "test:1", AlertKind::Test).await;
        let third = record(&host, "pipeline:2", AlertKind::Recovered).await;

        let page = fetch(&alert_host, 0, 20).await.unwrap();
        assert_eq!(
            page.alerts.iter().map(|a| a.seq).collect::<Vec<_>>(),
            [first, third]
        );
        assert_eq!(page.latest_seq, third);
        assert_eq!(page.alerts[0].kind, "failed");
        assert_eq!(page.alerts[0].tone, "danger");

        let page = fetch(&alert_host, first, 20).await.unwrap();
        assert_eq!(page.alerts.len(), 1);
        assert_eq!(page.alerts[0].key, "pipeline:2");
    }

    #[tokio::test]
    async fn fetch_refuses_plain_http_to_a_public_address() {
        let host = AlertHost {
            server_url: "http://8.8.8.8:8080".to_owned(),
            token: "t".to_owned(),
            allow_private_http: true,
            device_id: "d".to_owned(),
            host_name: "Public".to_owned(),
            standalone: true,
            notified_seq: 0,
            push: false,
        };

        assert!(matches!(
            fetch(&host, 0, 1).await,
            Err(RemoteError::PublicHttp)
        ));
    }

    #[tokio::test]
    async fn the_first_catch_up_stores_the_position_and_shows_nothing() {
        let host = start_host().await;
        let alert_host = pair(&host).await;
        let latest = record(&host, "pipeline:1", AlertKind::Failed).await;
        let session = bridge(alert_host);

        let plan = plan_catch_up(&session, None, 100, 10).await.unwrap();

        assert!(plan.alerts.is_empty());
        assert_eq!(plan.seen, latest);
        assert!(!plan.lower);
    }

    #[tokio::test]
    async fn a_later_catch_up_returns_what_was_missed_and_a_reset_host_lowers_the_position() {
        let host = start_host().await;
        let alert_host = pair(&host).await;
        let first = record(&host, "pipeline:1", AlertKind::Failed).await;
        let second = record(&host, "pipeline:2", AlertKind::Degraded).await;
        let session = bridge(alert_host);

        let plan = plan_catch_up(&session, Some(first), 100, 10).await.unwrap();
        assert_eq!(plan.alerts.len(), 1);
        assert_eq!(plan.alerts[0].seq, second);
        assert_eq!(plan.seen, second);

        let plan = plan_catch_up(&session, Some(second + 50), 100, 10)
            .await
            .unwrap();
        assert!(plan.alerts.is_empty());
        assert_eq!(plan.seen, second);
        assert!(plan.lower);
    }

    #[tokio::test]
    async fn more_than_a_page_of_missed_alerts_is_drained_before_the_session_counts_as_synced() {
        let host = start_host().await;
        let alert_host = pair(&host).await;
        let first = record(&host, "pipeline:0", AlertKind::Failed).await;
        for n in 1..=105 {
            record(&host, &format!("pipeline:{n}"), AlertKind::Failed).await;
        }
        let dir = tempfile::tempdir().unwrap();
        let remote = client_remote(Some(first));
        let session = bridge(alert_host);
        assert!(remote.attach(0, Arc::clone(&session)));

        let taken = take(&session, &remote, dir.path(), |_| {}).await.unwrap();

        assert_eq!(taken.alerts.len(), 105);
        assert_eq!(taken.alerts[0].seq, first + 1);
        assert_eq!(taken.alerts[104].seq, first + 105);
        assert!(taken.complete);
        assert_eq!(remote.lock().saved.alerts_seen, Some(first + 105));
        assert!(session.alerts_synced());
    }

    #[tokio::test]
    async fn hitting_the_page_cap_keeps_the_session_unsynced_so_the_next_pull_continues() {
        let host = start_host().await;
        let alert_host = pair(&host).await;
        let first = record(&host, "pipeline:0", AlertKind::Failed).await;
        for n in 1..=7 {
            record(&host, &format!("pipeline:{n}"), AlertKind::Failed).await;
        }
        let dir = tempfile::tempdir().unwrap();
        let remote = client_remote(Some(first));
        let session = bridge(alert_host);
        assert!(remote.attach(0, Arc::clone(&session)));

        let taken = take_pages(&session, &remote, dir.path(), 3, 2, |_| {})
            .await
            .unwrap();
        assert_eq!(taken.alerts.len(), 6);
        assert!(!taken.complete);
        assert_eq!(remote.lock().saved.alerts_seen, Some(first + 6));
        assert!(!session.alerts_synced());
        advance_live(&session, &remote, dir.path(), first + 7);
        assert_eq!(remote.lock().saved.alerts_seen, Some(first + 6));

        let rest = take_pages(&session, &remote, dir.path(), 3, 2, |_| {})
            .await
            .unwrap();
        assert_eq!(rest.alerts.len(), 1);
        assert!(rest.complete);
        assert_eq!(remote.lock().saved.alerts_seen, Some(first + 7));
        assert!(session.alerts_synced());
    }

    fn client_remote(seen: Option<i64>) -> RemoteConnection {
        let remote = RemoteConnection::new(SavedConnection::default());
        remote.lock().saved = SavedConnection {
            mode: SavedMode::Client {
                server_url: "http://127.0.0.1:1".to_owned(),
                device_id: "d1".to_owned(),
                host_kind: store::HostKind::Desktop,
                host_name: None,
            },
            allow_private_http: true,
            alerts_seen: seen,
            preferences_pending: false,
        };
        remote
    }

    #[tokio::test]
    async fn a_live_alert_cannot_move_the_position_before_the_missed_ones_are_taken() {
        let host = start_host().await;
        let alert_host = pair(&host).await;
        let first = record(&host, "pipeline:1", AlertKind::Failed).await;
        let second = record(&host, "pipeline:2", AlertKind::Degraded).await;
        let dir = tempfile::tempdir().unwrap();
        let remote = client_remote(Some(first));
        let session = bridge(alert_host.clone());
        assert!(remote.attach(0, Arc::clone(&session)));

        advance_live(&session, &remote, dir.path(), second);
        assert_eq!(remote.lock().saved.alerts_seen, Some(first));

        let taken = take(&session, &remote, dir.path(), |_| {}).await.unwrap();
        assert_eq!(
            taken.alerts.iter().map(|a| a.seq).collect::<Vec<_>>(),
            [second]
        );
        assert_eq!(remote.lock().saved.alerts_seen, Some(second));
        assert!(session.alerts_synced());

        let third = record(&host, "pipeline:3", AlertKind::Failed).await;
        advance_live(&session, &remote, dir.path(), third);
        assert_eq!(remote.lock().saved.alerts_seen, Some(third));

        let reconnected = bridge(alert_host);
        assert!(!reconnected.alerts_synced());
        advance_live(&reconnected, &remote, dir.path(), third + 5);
        assert_eq!(remote.lock().saved.alerts_seen, Some(third));
    }

    #[tokio::test]
    async fn concurrent_takes_return_each_missed_alert_once() {
        let host = start_host().await;
        let alert_host = pair(&host).await;
        let first = record(&host, "pipeline:1", AlertKind::Failed).await;
        record(&host, "pipeline:2", AlertKind::Failed).await;
        let dir = tempfile::tempdir().unwrap();
        let remote = client_remote(Some(first));
        let session = bridge(alert_host);
        assert!(remote.attach(0, Arc::clone(&session)));

        let (a, b) = tokio::join!(
            take(&session, &remote, dir.path(), |_| {}),
            take(&session, &remote, dir.path(), |_| {})
        );

        assert_eq!(a.unwrap().alerts.len() + b.unwrap().alerts.len(), 1);
    }

    #[tokio::test]
    async fn the_first_take_on_a_new_install_shows_nothing_and_stores_the_latest() {
        let host = start_host().await;
        let alert_host = pair(&host).await;
        let latest = record(&host, "pipeline:1", AlertKind::Failed).await;
        let dir = tempfile::tempdir().unwrap();
        let remote = client_remote(None);
        let session = bridge(alert_host);
        assert!(remote.attach(0, Arc::clone(&session)));

        let taken = take(&session, &remote, dir.path(), |_| {}).await.unwrap();

        assert!(taken.alerts.is_empty());
        assert_eq!(remote.lock().saved.alerts_seen, Some(latest));
    }

    #[tokio::test]
    async fn claiming_pairs_a_standalone_alert_host_and_revoking_removes_the_device() {
        let host = start_host().await;
        let latest = record(&host, "pipeline:1", AlertKind::Failed).await;

        let alert_host = pair(&host).await;

        assert!(alert_host.standalone);
        assert!(!alert_host.push);
        assert_eq!(alert_host.notified_seq, latest);
        assert_eq!(alert_host.host_name, "Studio");
        let devices = host.core.devices().list().await.unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, alert_host.device_id);

        revoke_device(&alert_host).await;

        assert!(host.core.devices().list().await.unwrap().is_empty());
        assert!(fetch(&alert_host, 0, 1).await.is_err());
    }
}
