use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{
    AtomicU64,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
    MutexGuard,
    PoisonError,
};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use futures::{
    SinkExt,
    StreamExt,
};
use pipedash_core::domain::pairing::is_private_ip;
use pipedash_core::domain::ServerUrl;
use rustls::ClientConfig;
use rustls_platform_verifier::ConfigVerifierExt;
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::{
    json,
    Value,
};
use tauri::{
    AppHandle,
    Emitter,
    Manager,
};
use tokio::net::{
    lookup_host,
    TcpStream,
};
use tokio::sync::{
    mpsc,
    oneshot,
};
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{
    client_async_with_config,
    connect_async_tls_with_config,
    Connector,
    MaybeTlsStream,
    WebSocketStream,
};

use super::store::HostKind;
use super::{
    RemoteConnection,
    RemoteError,
    RemoteStatus,
};

pub(super) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(45);
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
const REVOKE_TIMEOUT: Duration = Duration::from_secs(5);
const OUTBOX_CAPACITY: usize = 64;
const MAX_MESSAGE_BYTES: usize = 128 * 1024 * 1024;
const BACKOFF_BASE_MS: f64 = 1000.0;
const BACKOFF_MAX_MS: f64 = 30_000.0;

pub const CONNECTION_STATUS_EVENT: &str = "connection-status";
pub const REMOTE_ACCESS_REVOKED_EVENT: &str = "remote-access-revoked";

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

type RpcResult = Result<(u16, Vec<u8>), RemoteError>;

pub enum ClientHello<'a> {
    Auth {
        token: &'a str,
    },
    Claim {
        code: &'a str,
        name: &'a str,
        platform: &'a str,
    },
}

impl ClientHello<'_> {
    fn frame(&self) -> String {
        match self {
            Self::Auth { token } => json!({ "type": "auth", "token": token }),
            Self::Claim {
                code,
                name,
                platform,
            } => json!({ "type": "claim", "code": code, "name": name, "platform": platform }),
        }
        .to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthOk {
    pub host: HostKind,
    pub name: Option<String>,
    pub device_id: Option<String>,
    pub token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthErrorPayload {
    code: String,
    message: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ServerFrame {
    AuthOk(AuthOk),
    AuthError { code: String, message: String },
    Response { id: u64, status: u16, body: Vec<u8> },
    Event { name: String, payload: Value },
}

#[derive(Deserialize)]
struct RawFrame {
    #[serde(rename = "type")]
    kind: String,
    id: Option<u64>,
    status: Option<u16>,
    body: Option<String>,
    #[serde(rename = "bodyBase64")]
    body_base64: Option<String>,
    #[serde(default)]
    payload: Value,
}

pub fn parse_server_frame(text: &str) -> Option<ServerFrame> {
    let raw: RawFrame = serde_json::from_str(text).ok()?;

    match raw.kind.as_str() {
        "response" => {
            let body = match (raw.body, raw.body_base64) {
                (Some(body), _) => body.into_bytes(),
                (None, Some(encoded)) => STANDARD.decode(encoded).ok()?,
                (None, None) => Vec::new(),
            };
            Some(ServerFrame::Response {
                id: raw.id?,
                status: raw.status?,
                body,
            })
        }
        "auth-ok" => serde_json::from_value(raw.payload)
            .ok()
            .map(ServerFrame::AuthOk),
        "auth-error" => serde_json::from_value::<AuthErrorPayload>(raw.payload)
            .ok()
            .map(|payload| ServerFrame::AuthError {
                code: payload.code,
                message: payload.message,
            }),
        _ => Some(ServerFrame::Event {
            name: raw.kind,
            payload: raw.payload,
        }),
    }
}

fn request_frame(id: u64, method: &str, path: &str, body: Option<&str>) -> Message {
    Message::Text(
        json!({ "type": "request", "id": id, "method": method, "path": path, "body": body })
            .to_string()
            .into(),
    )
}

fn revokes_access(code: &str) -> bool {
    matches!(code, "AUTH_FAILED" | "DEVICE_REMOVED")
}

fn socket_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES))
}

fn backoff(attempts: u32) -> Duration {
    let exponent = attempts.saturating_sub(1).min(20) as i32;
    let millis = (BACKOFF_BASE_MS * 1.5_f64.powi(exponent)).min(BACKOFF_MAX_MS);
    Duration::from_millis(millis as u64)
}

async fn await_auth(stream: &mut WsStream, host: &str) -> Result<AuthOk, RemoteError> {
    while let Some(Ok(message)) = stream.next().await {
        match message {
            Message::Text(text) => match parse_server_frame(&text) {
                Some(ServerFrame::AuthOk(ok)) => return Ok(ok),
                Some(ServerFrame::AuthError { code, message }) => {
                    return Err(RemoteError::Rejected { code, message })
                }
                _ => {}
            },
            Message::Close(_) => break,
            _ => {}
        }
    }

    Err(RemoteError::Unreachable(host.to_string()))
}

fn require_private(host: &str, addrs: &[SocketAddr]) -> Result<(), RemoteError> {
    if addrs.is_empty() {
        return Err(RemoteError::Unreachable(host.to_string()));
    }
    if addrs.iter().all(|addr| is_private_ip(addr.ip())) {
        Ok(())
    } else {
        Err(RemoteError::PublicHttp)
    }
}

async fn connect_private(url: &ServerUrl) -> Result<TcpStream, RemoteError> {
    let unreachable = || RemoteError::Unreachable(url.host.clone());
    let port = reqwest::Url::parse(&url.url)
        .ok()
        .and_then(|parsed| parsed.port_or_known_default())
        .ok_or_else(unreachable)?;
    let host = url.host.trim_start_matches('[').trim_end_matches(']');
    let addrs: Vec<SocketAddr> = lookup_host((host, port))
        .await
        .map_err(|error| {
            tracing::debug!(%error, host = %url.host, "Couldn't resolve the server address");
            unreachable()
        })?
        .collect();
    require_private(&url.host, &addrs)?;

    for addr in addrs {
        if let Ok(stream) = TcpStream::connect(addr).await {
            return Ok(stream);
        }
    }

    Err(unreachable())
}

async fn connect(url: &ServerUrl) -> Result<WsStream, RemoteError> {
    let unreachable = || RemoteError::Unreachable(url.host.clone());
    let opened = |error: tokio_tungstenite::tungstenite::Error| {
        tracing::debug!(%error, host = %url.host, "Couldn't open the server socket");
        unreachable()
    };

    if url.scheme == "https" {
        let config = ClientConfig::with_platform_verifier().map_err(|error| {
            tracing::warn!(%error, "Couldn't build the TLS configuration");
            unreachable()
        })?;
        let (stream, _) = connect_async_tls_with_config(
            url.websocket_url(),
            Some(socket_config()),
            false,
            Some(Connector::Rustls(Arc::new(config))),
        )
        .await
        .map_err(opened)?;

        return Ok(stream);
    }

    let tcp = connect_private(url).await?;
    let (stream, _) = client_async_with_config(
        url.websocket_url(),
        MaybeTlsStream::Plain(tcp),
        Some(socket_config()),
    )
    .await
    .map_err(opened)?;

    Ok(stream)
}

pub async fn open(
    url: &ServerUrl, hello: ClientHello<'_>,
) -> Result<(WsStream, AuthOk), RemoteError> {
    let unreachable = || RemoteError::Unreachable(url.host.clone());
    let mut stream = tokio::time::timeout(CONNECT_TIMEOUT, connect(url))
        .await
        .map_err(|_| unreachable())??;

    stream
        .send(Message::Text(hello.frame().into()))
        .await
        .map_err(|_| unreachable())?;
    let ok = tokio::time::timeout(AUTH_TIMEOUT, await_auth(&mut stream, &url.host))
        .await
        .map_err(|_| RemoteError::Timeout)??;

    Ok((stream, ok))
}

pub async fn revoke(mut stream: WsStream, device_id: &str) {
    let path = format!("/devices/{device_id}");
    let _ = tokio::time::timeout(REVOKE_TIMEOUT, async {
        if stream
            .send(request_frame(1, "DELETE", &path, None))
            .await
            .is_err()
        {
            return;
        }
        while let Some(Ok(message)) = stream.next().await {
            if let Message::Text(text) = message {
                if matches!(
                    parse_server_frame(&text),
                    Some(ServerFrame::Response { .. } | ServerFrame::AuthError { .. })
                ) {
                    return;
                }
            }
        }
    })
    .await;
    let _ = stream.close(None).await;
}

pub struct Session {
    host: String,
    outgoing: mpsc::Sender<Message>,
    pending: Mutex<HashMap<u64, oneshot::Sender<RpcResult>>>,
    next_id: AtomicU64,
}

impl Session {
    pub(super) fn new(host: &str, outgoing: mpsc::Sender<Message>) -> Self {
        Self {
            host: host.to_string(),
            outgoing,
            pending: Mutex::default(),
            next_id: AtomicU64::new(1),
        }
    }

    fn pending(&self) -> MutexGuard<'_, HashMap<u64, oneshot::Sender<RpcResult>>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub async fn request(
        &self, method: &str, path: &str, body: Option<&str>, timeout: Duration,
    ) -> RpcResult {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply, answer) = oneshot::channel();
        self.pending().insert(id, reply);

        let outcome = tokio::time::timeout(timeout, async {
            self.outgoing
                .send(request_frame(id, method, path, body))
                .await
                .map_err(|_| RemoteError::Unreachable(self.host.clone()))?;
            answer
                .await
                .unwrap_or_else(|_| Err(RemoteError::Unreachable(self.host.clone())))
        })
        .await;
        self.pending().remove(&id);

        outcome.unwrap_or(Err(RemoteError::Timeout))
    }

    fn resolve(&self, id: u64, status: u16, body: Vec<u8>) {
        if let Some(reply) = self.pending().remove(&id) {
            let _ = reply.send(Ok((status, body)));
        }
    }

    fn fail_all(&self) {
        let pending = std::mem::take(&mut *self.pending());
        for reply in pending.into_values() {
            let _ = reply.send(Err(RemoteError::Unreachable(self.host.clone())));
        }
    }
}

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
    }
}

fn handle_frame(app: &AppHandle, session: &Session, text: &str) -> Option<Outcome> {
    match parse_server_frame(text)? {
        ServerFrame::Response { id, status, body } => session.resolve(id, status, body),
        ServerFrame::AuthError { code, .. } if revokes_access(&code) => {
            return Some(Outcome::Removed)
        }
        ServerFrame::AuthError { .. } => return Some(Outcome::Lost),
        ServerFrame::AuthOk(_) => {}
        ServerFrame::Event { name, payload } => {
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

struct FailOnDrop(Arc<Session>);

impl Drop for FailOnDrop {
    fn drop(&mut self) {
        self.0.fail_all();
    }
}

async fn load_token() -> Result<Option<String>, RemoteError> {
    tauri::async_runtime::spawn_blocking(super::store::get_token)
        .await
        .map_err(|_| RemoteError::Keychain)?
}

pub async fn run(
    app: AppHandle, generation: u64, url: ServerUrl, token: Option<String>, first: Option<WsStream>,
) {
    let mut first = first;
    let mut token = token;
    loop {
        let stream = match first.take() {
            Some(stream) => Some(stream),
            None => {
                if token.is_none() {
                    match load_token().await {
                        Ok(Some(loaded)) => {
                            app.state::<RemoteConnection>()
                                .remember_token(generation, &loaded);
                            token = Some(loaded);
                        }
                        Ok(None) => {
                            remove(&app, generation);
                            return;
                        }
                        Err(error) => tracing::debug!(%error, "Couldn't read the device token"),
                    }
                }
                match token.as_deref() {
                    Some(token) => match open(&url, ClientHello::Auth { token }).await {
                        Ok((stream, _)) => Some(stream),
                        Err(RemoteError::Rejected { code, .. }) if revokes_access(&code) => {
                            remove(&app, generation);
                            return;
                        }
                        Err(error) => {
                            tracing::debug!(%error, "Couldn't reach the server");
                            None
                        }
                    },
                    None => None,
                }
            }
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pipedash_core::infrastructure::{
        PipedashConfig,
        StorageBackendType,
        StorageManager,
    };
    use pipedash_core::CoreContext;
    use pipedash_web::pairing::create_pairing;
    use pipedash_web::state::AppState;
    use pipedash_web::ws::WebSocketEventBus;
    use tokio::sync::broadcast;

    use super::*;

    #[test]
    fn a_response_carries_its_text_or_base64_body() {
        assert_eq!(
            parse_server_frame(r#"{"type":"response","id":7,"status":200,"body":"{\"ok\":true}"}"#),
            Some(ServerFrame::Response {
                id: 7,
                status: 200,
                body: br#"{"ok":true}"#.to_vec(),
            })
        );
        assert_eq!(
            parse_server_frame(r#"{"type":"response","id":8,"status":200,"bodyBase64":"AP8="}"#),
            Some(ServerFrame::Response {
                id: 8,
                status: 200,
                body: vec![0, 255],
            })
        );
        assert_eq!(
            parse_server_frame(r#"{"type":"response","id":9,"status":204,"body":""}"#),
            Some(ServerFrame::Response {
                id: 9,
                status: 204,
                body: Vec::new(),
            })
        );
        assert_eq!(
            parse_server_frame(r#"{"type":"response","id":9,"status":200,"bodyBase64":"%"}"#),
            None
        );
    }

    #[test]
    fn auth_frames_keep_the_server_code_and_message() {
        assert_eq!(
            parse_server_frame(
                r#"{"type":"auth-error","payload":{"code":"HOST_LOCKED","message":"Locked"}}"#
            ),
            Some(ServerFrame::AuthError {
                code: "HOST_LOCKED".to_string(),
                message: "Locked".to_string(),
            })
        );
        assert_eq!(
            parse_server_frame(
                r#"{"type":"auth-ok","payload":{"host":"desktop","name":"Studio","deviceId":"d1","token":"t"}}"#
            ),
            Some(ServerFrame::AuthOk(AuthOk {
                host: HostKind::Desktop,
                name: Some("Studio".to_string()),
                device_id: Some("d1".to_string()),
                token: Some("t".to_string()),
            }))
        );
        assert_eq!(
            parse_server_frame(
                r#"{"type":"auth-ok","payload":{"host":"server","name":null,"deviceId":null}}"#
            ),
            Some(ServerFrame::AuthOk(AuthOk {
                host: HostKind::Server,
                name: None,
                device_id: None,
                token: None,
            }))
        );
    }

    #[test]
    fn other_frames_are_events_and_garbage_is_ignored() {
        assert_eq!(
            parse_server_frame(r#"{"type":"pipelines-updated","payload":{"provider_id":3}}"#),
            Some(ServerFrame::Event {
                name: "pipelines-updated".to_string(),
                payload: json!({ "provider_id": 3 }),
            })
        );
        for garbage in [
            "",
            "nope",
            "[]",
            r#"{"payload":{}}"#,
            r#"{"type":"response"}"#,
        ] {
            assert_eq!(parse_server_frame(garbage), None, "{garbage}");
        }
    }

    #[test]
    fn only_a_rejected_device_loses_access() {
        assert!(revokes_access("AUTH_FAILED"));
        assert!(revokes_access("DEVICE_REMOVED"));
        for code in ["HOST_LOCKED", "SERVER_NOT_READY", "AUTH_TIMEOUT", ""] {
            assert!(!revokes_access(code), "{code}");
        }
    }

    #[test]
    fn reconnects_back_off_by_half_again_up_to_thirty_seconds() {
        let millis: Vec<u128> = (0..=9).map(|n| backoff(n).as_millis()).collect();
        assert_eq!(
            millis,
            [1000, 1000, 1500, 2250, 3375, 5062, 7593, 11390, 17085, 25628]
        );
        assert_eq!(backoff(10).as_millis(), 30_000);
        assert_eq!(backoff(u32::MAX).as_millis(), 30_000);
    }

    #[tokio::test]
    async fn a_request_waits_for_its_own_response() {
        let (outgoing, mut queue) = mpsc::channel(4);
        let session = Arc::new(Session::new("host", outgoing));

        let asking = Arc::clone(&session);
        let first = tokio::spawn(async move {
            asking
                .request("GET", "/a", None, Duration::from_secs(5))
                .await
        });
        let asking = Arc::clone(&session);
        let second = tokio::spawn(async move {
            asking
                .request("POST", "/b", Some("{}"), Duration::from_secs(5))
                .await
        });

        for _ in 0..2 {
            let Some(Message::Text(text)) = queue.recv().await else {
                panic!("no request frame");
            };
            let frame: Value = serde_json::from_str(&text).unwrap();
            let id = frame["id"].as_u64().unwrap();
            let path = frame["path"].as_str().unwrap().to_string();
            session.resolve(id, 200, path.into_bytes());
        }

        assert_eq!(first.await.unwrap().unwrap(), (200, b"/a".to_vec()));
        assert_eq!(second.await.unwrap().unwrap(), (200, b"/b".to_vec()));
    }

    #[tokio::test]
    async fn an_unanswered_request_times_out_and_a_lost_socket_fails_it() {
        let (outgoing, queue) = mpsc::channel(4);
        let session = Arc::new(Session::new("host", outgoing));

        let result = session
            .request("GET", "/slow", None, Duration::from_millis(20))
            .await;
        assert!(matches!(result, Err(RemoteError::Timeout)));
        assert!(session.pending().is_empty());

        let waiting = Arc::clone(&session);
        let pending = tokio::spawn(async move {
            waiting
                .request("GET", "/later", None, Duration::from_secs(5))
                .await
        });
        while session.pending().is_empty() {
            tokio::task::yield_now().await;
        }
        session.fail_all();
        assert!(matches!(
            pending.await.unwrap(),
            Err(RemoteError::Unreachable(host)) if host == "host"
        ));

        drop(queue);
        let result = session
            .request("GET", "/gone", None, Duration::from_secs(5))
            .await;
        assert!(matches!(result, Err(RemoteError::Unreachable(_))));
        assert!(session.pending().is_empty());
    }

    #[tokio::test]
    async fn a_full_queue_counts_against_the_request_deadline() {
        let (outgoing, _queue) = mpsc::channel(1);
        let session = Session::new("host", outgoing);
        session
            .outgoing
            .send(request_frame(0, "GET", "/fill", None))
            .await
            .unwrap();

        let started = tokio::time::Instant::now();
        let result = session
            .request("GET", "/blocked", None, Duration::from_millis(50))
            .await;

        assert!(matches!(result, Err(RemoteError::Timeout)));
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(session.pending().is_empty());
    }

    #[tokio::test]
    async fn dropping_the_serving_task_fails_the_requests_waiting_on_it() {
        let (outgoing, mut queue) = mpsc::channel(4);
        let session = Arc::new(Session::new("host", outgoing));
        let guard = FailOnDrop(Arc::clone(&session));

        let waiting = Arc::clone(&session);
        let pending = tokio::spawn(async move {
            waiting
                .request("GET", "/hang", None, Duration::from_secs(60))
                .await
        });
        queue.recv().await.unwrap();
        drop(guard);

        assert!(matches!(
            tokio::time::timeout(Duration::from_secs(2), pending)
                .await
                .unwrap()
                .unwrap(),
            Err(RemoteError::Unreachable(host)) if host == "host"
        ));
    }

    fn addrs(list: &[&str]) -> Vec<SocketAddr> {
        list.iter().map(|addr| addr.parse().unwrap()).collect()
    }

    #[test]
    fn plain_http_needs_every_resolved_address_to_be_private() {
        assert!(require_private("nas", &addrs(&["192.168.1.20:8080"])).is_ok());
        assert!(require_private("nas", &addrs(&["10.0.0.5:80", "[fd00::1]:80"])).is_ok());
        assert!(require_private("nas", &addrs(&["127.0.0.1:80"])).is_ok());

        for public in [
            &["8.8.8.8:80"][..],
            &["192.168.1.20:80", "8.8.8.8:80"][..],
            &["[2001:4860:4860::8888]:80"][..],
            &["[::ffff:8.8.8.8]:80"][..],
        ] {
            assert!(
                matches!(
                    require_private("nas", &addrs(public)),
                    Err(RemoteError::PublicHttp)
                ),
                "{public:?}"
            );
        }
        assert!(matches!(
            require_private("nas", &[]),
            Err(RemoteError::Unreachable(host)) if host == "nas"
        ));
    }

    #[tokio::test]
    async fn a_public_address_over_plain_http_is_refused_before_connecting() {
        let url = ServerUrl::parse("http://8.8.8.8:8080").unwrap();

        let Err(error) = open(&url, ClientHello::Auth { token: "t" }).await else {
            panic!("a public address must not get the token");
        };
        assert!(matches!(error, RemoteError::PublicHttp));
    }

    struct Host {
        url: ServerUrl,
        state: AppState,
        _storage: StorageManager,
        _dir: tempfile::TempDir,
    }

    async fn start_host() -> Host {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("config.toml"),
            format!(
                "[storage]\ndata_dir = {:?}\n",
                dir.path().display().to_string()
            ),
        )
        .unwrap();
        std::env::set_var("PIPEDASH_CONFIG_PATH", dir.path().join("config.toml"));
        let mut config = PipedashConfig::default();
        config.storage.backend = StorageBackendType::Sqlite;
        config.storage.data_dir = dir.path().display().to_string();
        config.storage.vault_password = Some("vault-password".into());
        let storage = StorageManager::from_config(config, false).await.unwrap();

        let bus = Arc::new(WebSocketEventBus::from_sender(broadcast::channel(64).0));
        let core = CoreContext::with_storage_manager(&storage, bus.clone())
            .await
            .unwrap();
        let state = AppState::hosted(bus, Some(core), "Studio");

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = ServerUrl::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        tokio::spawn(pipedash_web::serve_host(
            listener,
            state.clone(),
            std::future::pending(),
        ));

        Host {
            url,
            state,
            _storage: storage,
            _dir: dir,
        }
    }

    fn claim(code: &str) -> ClientHello<'_> {
        ClientHello::Claim {
            code,
            name: "Test phone",
            platform: "ios",
        }
    }

    #[tokio::test]
    async fn a_code_pairs_one_device_and_a_revoked_device_is_turned_away() {
        let host = start_host().await;
        let code = create_pairing(&host.state, &host.url.url).unwrap().code;

        let (stream, ok) = open(&host.url, claim(&code)).await.unwrap();
        assert_eq!(ok.host, HostKind::Desktop);
        assert_eq!(ok.name.as_deref(), Some("Studio"));
        let token = ok.token.expect("a claim returns the device token");
        let device_id = ok.device_id.expect("a claim returns the device id");

        let Err(second) = open(&host.url, claim(&code)).await else {
            panic!("a pairing code works once");
        };
        assert!(matches!(
            second,
            RemoteError::Rejected { ref code, .. } if code == "PAIRING_CODE_INVALID"
        ));

        let (_, again) = open(&host.url, ClientHello::Auth { token: &token })
            .await
            .unwrap();
        assert_eq!(again.device_id.as_deref(), Some(device_id.as_str()));
        assert_eq!(again.token, None);

        revoke(stream, &device_id).await;
        let Err(rejected) = open(&host.url, ClientHello::Auth { token: &token }).await else {
            panic!("a revoked device can't sign in");
        };
        assert!(matches!(
            rejected,
            RemoteError::Rejected { ref code, .. } if code == "AUTH_FAILED"
        ));
    }

    #[tokio::test]
    async fn an_unknown_address_is_unreachable() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = ServerUrl::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        drop(listener);

        let Err(error) = open(&url, ClientHello::Auth { token: "t" }).await else {
            panic!("nothing listens there");
        };
        assert!(matches!(error, RemoteError::Unreachable(host) if host == "127.0.0.1"));
    }
}
