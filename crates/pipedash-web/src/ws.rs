use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::{
    body::{
        to_bytes,
        Body,
        Bytes,
    },
    extract::{
        ws::{
            Message,
            WebSocket,
            WebSocketUpgrade,
        },
        State,
    },
    http::{
        header::{
            AUTHORIZATION,
            CONTENT_TYPE,
        },
        Request,
    },
    response::IntoResponse,
    Router,
};
use base64::{
    engine::general_purpose::STANDARD,
    Engine,
};
use futures_util::{
    stream::SplitSink,
    SinkExt,
    StreamExt,
};
use pipedash_core::domain::pairing::validate_device;
use pipedash_core::event::{
    CoreEvent,
    EventBus,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::json;
use tokio::sync::{
    broadcast,
    mpsc,
    watch,
    Semaphore,
};
use tokio::task::JoinSet;
use tower::ServiceExt;

use crate::state::AppState;

const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const PING_INTERVAL: Duration = Duration::from_secs(20);
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_IN_FLIGHT: usize = 16;
const OUTBOX_CAPACITY: usize = 64;
const MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;

const AUTH_FAILED_MESSAGE: &str = "This device isn't paired with this Pipedash.";
const RESPONSE_TOO_LARGE: &str = r#"{"code":"RESPONSE_TOO_LARGE","message":"The response is too large to send to this device."}"#;
const INVALID_PATH: &str = r#"{"code":"BAD_REQUEST","message":"Invalid request path."}"#;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum ClientFrame {
    Auth {
        token: String,
    },
    Claim {
        code: String,
        name: String,
        platform: String,
    },
    Request {
        id: u64,
        method: String,
        path: String,
        #[serde(default)]
        body: Option<String>,
    },
}

#[derive(Serialize)]
struct ResponseFrame<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    id: u64,
    status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<&'a str>,
    #[serde(rename = "bodyBase64", skip_serializing_if = "Option::is_none")]
    body_base64: Option<String>,
}

enum Identity {
    Anonymous,
    Operator(String),
    Device { id: String, token: String },
}

pub struct WebSocketEventBus {
    tx: broadcast::Sender<CoreEvent>,
}

impl WebSocketEventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self { tx }
    }

    pub fn from_sender(tx: broadcast::Sender<CoreEvent>) -> Self {
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.tx.subscribe()
    }
}

impl Default for WebSocketEventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventBus for WebSocketEventBus {
    async fn emit(&self, event: CoreEvent) {
        let _ = self.tx.send(event);
    }

    async fn emit_to(&self, _target: &str, event: CoreEvent) {
        let _ = self.tx.send(event);
    }
}

fn event_frame(event: &CoreEvent) -> Option<String> {
    serde_json::to_string(&json!({
        "type": event.event_name(),
        "payload": event.to_json_payload(),
    }))
    .ok()
}

fn text(value: serde_json::Value) -> Message {
    Message::Text(value.to_string().into())
}

fn response_frame(id: u64, status: u16, body: &[u8]) -> Message {
    let (body, body_base64) = match std::str::from_utf8(body) {
        Ok(text) => (Some(text), None),
        Err(_) => (None, Some(STANDARD.encode(body))),
    };
    let frame = ResponseFrame {
        kind: "response",
        id,
        status,
        body,
        body_base64,
    };

    Message::Text(serde_json::to_string(&frame).unwrap_or_default().into())
}

async fn dispatch(
    router: Router, method: &str, path: &str, body: Option<String>, token: Option<&str>,
) -> (u16, Bytes) {
    let invalid = || (400, Bytes::from_static(INVALID_PATH.as_bytes()));
    if !path.starts_with('/') || path.starts_with("//") {
        return invalid();
    }

    let mut request = Request::builder()
        .method(method)
        .uri(format!("/api/v1{path}"));
    if let Some(token) = token {
        request = request.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    if body.is_some() {
        request = request.header(CONTENT_TYPE, "application/json");
    }
    let Ok(request) = request.body(body.map_or_else(Body::empty, Body::from)) else {
        return invalid();
    };

    let Ok(response) = router.oneshot(request).await;
    let (parts, body) = response.into_parts();
    match to_bytes(body, MAX_RESPONSE_BYTES).await {
        Ok(bytes) => (parts.status.as_u16(), bytes),
        Err(_) => (500, Bytes::from_static(RESPONSE_TOO_LARGE.as_bytes())),
    }
}

async fn write_loop(
    mut sender: SplitSink<WebSocket, Message>, mut outbox: mpsc::Receiver<Message>,
) {
    let mut ping =
        tokio::time::interval_at(tokio::time::Instant::now() + PING_INTERVAL, PING_INTERVAL);
    loop {
        let message = tokio::select! {
            message = outbox.recv() => match message {
                Some(message) => message,
                None => break,
            },
            _ = ping.tick() => Message::Ping(Bytes::new()),
        };
        let closing = matches!(message, Message::Close(_));
        if !matches!(
            tokio::time::timeout(WRITE_TIMEOUT, sender.send(message)).await,
            Ok(Ok(()))
        ) || closing
        {
            break;
        }
    }
}

async fn next_event(
    events: &mut Option<broadcast::Receiver<CoreEvent>>,
) -> Result<CoreEvent, broadcast::error::RecvError> {
    match events {
        Some(events) => events.recv().await,
        None => std::future::pending().await,
    }
}

fn touch_device(state: &AppState, id: String) {
    let state = state.clone();
    tokio::spawn(async move {
        let Ok(core) = state.core().await else {
            return;
        };
        if let Err(error) = core.devices().touch(&id).await {
            tracing::warn!(%error, "Failed to record device activity");
        }
    });
}

struct Connection {
    state: AppState,
    out: mpsc::Sender<Message>,
    identity: Option<Identity>,
    events: Option<broadcast::Receiver<CoreEvent>>,
    router: Option<Router>,
    permits: Arc<Semaphore>,
    requests: JoinSet<()>,
}

impl Connection {
    fn new(state: AppState, out: mpsc::Sender<Message>) -> Self {
        let identity = (!state.requires_auth()).then_some(Identity::Anonymous);
        let events = identity.as_ref().map(|_| state.ws_event_bus.subscribe());

        Self {
            state,
            out,
            identity,
            events,
            router: None,
            permits: Arc::new(Semaphore::new(MAX_IN_FLIGHT)),
            requests: JoinSet::new(),
        }
    }

    async fn reject(&self, code: &str, message: &str) {
        let _ = self
            .out
            .send(text(json!({
                "type": "auth-error",
                "payload": { "code": code, "message": message },
            })))
            .await;
        let _ = self.out.send(Message::Close(None)).await;
    }

    async fn authenticated(&mut self, identity: Identity, issued_token: Option<&str>) -> bool {
        let device_id = match &identity {
            Identity::Device { id, .. } => Some(id.as_str()),
            _ => None,
        };
        let mut payload = json!({
            "host": if self.state.desktop_host.is_some() { "desktop" } else { "server" },
            "name": self.state.desktop_host.as_deref(),
            "deviceId": device_id,
        });
        if let Some(token) = issued_token {
            payload["token"] = token.into();
        }

        if self.events.is_none() {
            self.events = Some(self.state.ws_event_bus.subscribe());
        }
        self.identity = Some(identity);
        self.router = None;

        self.out
            .send(text(json!({ "type": "auth-ok", "payload": payload })))
            .await
            .is_ok()
    }

    async fn on_text(&mut self, raw: &str) -> bool {
        let Ok(frame) = serde_json::from_str::<ClientFrame>(raw) else {
            if self.identity.is_none() {
                self.reject("AUTH_FAILED", AUTH_FAILED_MESSAGE).await;
                return false;
            }
            tracing::trace!("Ignoring an unrecognized WebSocket frame");
            return true;
        };

        match frame {
            ClientFrame::Auth { token } => self.on_auth(&token).await,
            ClientFrame::Claim {
                code,
                name,
                platform,
            } => self.on_claim(&code, &name, &platform).await,
            ClientFrame::Request {
                id,
                method,
                path,
                body,
            } => self.on_request(id, method, path, body).await,
        }
    }

    async fn on_auth(&mut self, token: &str) -> bool {
        if self.events.is_none() {
            self.events = Some(self.state.ws_event_bus.subscribe());
        }
        let anonymous = matches!(self.identity, Some(Identity::Anonymous));

        let identity = if self
            .state
            .api_token()
            .is_some_and(|expected| crate::token_matches(token, &expected))
        {
            Identity::Operator(token.to_string())
        } else {
            match self.state.paired_device(token).await {
                Ok(Some(device)) => {
                    touch_device(&self.state, device.id.clone());
                    Identity::Device {
                        id: device.id,
                        token: token.to_string(),
                    }
                }
                Ok(None) if anonymous => Identity::Anonymous,
                Ok(None) => {
                    if self.state.desktop_host.is_some() && self.state.core().await.is_err() {
                        self.reject("HOST_LOCKED", crate::HOST_LOCKED_MESSAGE).await;
                    } else {
                        self.reject("AUTH_FAILED", AUTH_FAILED_MESSAGE).await;
                    }
                    return false;
                }
                Err(_) => {
                    self.reject("SERVER_NOT_READY", crate::SERVER_NOT_READY_MESSAGE)
                        .await;
                    return false;
                }
            }
        };

        self.authenticated(identity, None).await
    }

    async fn on_claim(&mut self, code: &str, name: &str, platform: &str) -> bool {
        let Ok(core) = self.state.core().await else {
            if self.state.desktop_host.is_some() {
                self.reject("HOST_LOCKED", crate::HOST_LOCKED_MESSAGE).await;
            } else {
                self.reject("SERVER_NOT_READY", crate::SERVER_NOT_READY_MESSAGE)
                    .await;
            }
            return false;
        };
        let name = match validate_device(name, platform) {
            Ok(name) => name,
            Err((code, message)) => {
                self.reject(code, message).await;
                return false;
            }
        };
        if !self.state.pairing_codes.take(code) {
            self.reject(
                "PAIRING_CODE_INVALID",
                "This code expired or was already used. Create a new code on the server.",
            )
            .await;
            return false;
        }

        match core.pair_device(&name, platform).await {
            Ok((device, token)) => {
                let identity = Identity::Device {
                    id: device.id,
                    token: token.clone(),
                };
                self.authenticated(identity, Some(&token)).await
            }
            Err(error) => {
                tracing::warn!(%error, "Failed to pair a device");
                self.reject(
                    "PAIRING_FAILED",
                    "Pipedash couldn't pair this device. Try again.",
                )
                .await;
                false
            }
        }
    }

    async fn on_request(
        &mut self, id: u64, method: String, path: String, body: Option<String>,
    ) -> bool {
        let token = match &self.identity {
            None => {
                self.reject("AUTH_FAILED", AUTH_FAILED_MESSAGE).await;
                return false;
            }
            Some(Identity::Anonymous) => None,
            Some(Identity::Operator(token)) | Some(Identity::Device { token, .. }) => {
                Some(token.clone())
            }
        };

        let state = &self.state;
        let router = self
            .router
            .get_or_insert_with(|| crate::host_router(state.clone()))
            .clone();
        let permits = Arc::clone(&self.permits);
        let out = self.out.clone();
        self.requests.spawn(async move {
            let Ok(_permit) = permits.acquire_owned().await else {
                return;
            };
            let (status, body) = dispatch(router, &method, &path, body, token.as_deref()).await;
            let _ = out.send(response_frame(id, status, &body)).await;
        });

        true
    }

    async fn on_event(&mut self, event: Result<CoreEvent, broadcast::error::RecvError>) -> bool {
        match event {
            Ok(event) => {
                if let (CoreEvent::DeviceRevoked { id }, Some(Identity::Device { id: own, .. })) =
                    (&event, &self.identity)
                {
                    if id == own {
                        self.reject("DEVICE_REMOVED", "This device was removed.")
                            .await;
                        return false;
                    }
                }

                match event_frame(&event) {
                    Some(json) => self.out.send(Message::Text(json.into())).await.is_ok(),
                    None => true,
                }
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "WebSocket client fell behind, closing");
                let _ = self.out.send(Message::Close(None)).await;
                false
            }
            Err(broadcast::error::RecvError::Closed) => false,
        }
    }
}

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn closed(closing: &mut watch::Receiver<bool>) {
    let _ = closing.wait_for(|closed| *closed).await;
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (sender, mut receiver) = socket.split();
    let (out, outbox) = mpsc::channel(OUTBOX_CAPACITY);
    let writer = tokio::spawn(write_loop(sender, outbox));
    let mut conn = Connection::new(state, out);
    let mut closing = conn.state.closing.subscribe();
    let auth_deadline = tokio::time::sleep(AUTH_TIMEOUT);
    tokio::pin!(auth_deadline);

    loop {
        tokio::select! {
            _ = conn.out.closed() => break,
            _ = closed(&mut closing) => {
                let _ = conn.out.send(Message::Close(None)).await;
                break;
            }
            frame = receiver.next() => match frame {
                Some(Ok(Message::Text(raw))) => {
                    if !conn.on_text(&raw).await {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | None => {
                    tracing::debug!("WebSocket client disconnected");
                    break;
                }
                Some(Err(error)) => {
                    tracing::warn!("WebSocket error: {}", error);
                    break;
                }
                Some(Ok(_)) => {}
            },
            event = next_event(&mut conn.events) => {
                if !conn.on_event(event).await {
                    break;
                }
            }
            Some(_) = conn.requests.join_next(), if !conn.requests.is_empty() => {}
            _ = &mut auth_deadline, if conn.identity.is_none() => {
                conn.reject("AUTH_TIMEOUT", "Authentication timed out.").await;
                break;
            }
        }
    }

    drop(conn);
    let _ = writer.await;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use axum::routing::get;
    use axum::Router;
    use serde_json::{
        json,
        Value,
    };
    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };
    use tokio::net::{
        TcpListener,
        TcpStream,
    };

    use super::*;
    use crate::test_support::hosted;

    async fn read_frame(stream: &mut TcpStream) -> Option<(u8, Vec<u8>)> {
        let mut head = [0u8; 2];
        stream.read_exact(&mut head).await.ok()?;
        let len = match head[1] & 0x7f {
            126 => stream.read_u16().await.ok()? as usize,
            127 => stream.read_u64().await.ok()? as usize,
            len => len as usize,
        };
        let mut payload = vec![0u8; len];
        stream.read_exact(&mut payload).await.ok()?;
        Some((head[0] & 0x0f, payload))
    }

    async fn upgrade(addr: std::net::SocketAddr, path: &str) -> TcpStream {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let handshake = format!(
            "GET {path} HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
        );
        stream.write_all(handshake.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        while !response.ends_with(b"\r\n\r\n") {
            response.push(stream.read_u8().await.unwrap());
        }
        assert!(response.starts_with(b"HTTP/1.1 101"));
        tokio::time::sleep(Duration::from_millis(100)).await;

        stream
    }

    async fn connect_state(state: AppState) -> TcpStream {
        let app = Router::new()
            .route("/ws", get(ws_handler))
            .with_state(state);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        upgrade(addr, "/ws").await
    }

    async fn connect(bus: &Arc<WebSocketEventBus>) -> TcpStream {
        connect_state(AppState::setup_mode(bus.clone())).await
    }

    async fn send_json(stream: &mut TcpStream, value: Value) {
        let payload = value.to_string().into_bytes();
        let mask = [7u8, 11, 13, 17];
        let mut frame = vec![0x81];
        match payload.len() {
            len if len < 126 => frame.push(0x80 | len as u8),
            len => {
                frame.push(0x80 | 126);
                frame.extend((len as u16).to_be_bytes());
            }
        }
        frame.extend(mask);
        frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        stream.write_all(&frame).await.unwrap();
    }

    async fn next_message(stream: &mut TcpStream) -> Value {
        let (opcode, payload) = tokio::time::timeout(Duration::from_secs(5), read_frame(stream))
            .await
            .expect("no message arrived")
            .expect("the socket closed");
        assert_eq!(opcode, 0x1);
        serde_json::from_slice(&payload).unwrap()
    }

    async fn assert_closed(stream: &mut TcpStream) {
        let closed = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match read_frame(stream).await {
                    None | Some((0x8, _)) => break,
                    Some(_) => {}
                }
            }
        })
        .await;
        assert!(closed.is_ok(), "the socket stayed open");
    }

    async fn claim(stream: &mut TcpStream, code: &str) -> Value {
        send_json(
            stream,
            json!({ "type": "claim", "code": code, "name": "Pixel", "platform": "android" }),
        )
        .await;
        next_message(stream).await
    }

    #[tokio::test]
    async fn lagged_client_gets_closed_so_it_reconnects() {
        let bus = Arc::new(WebSocketEventBus::new());
        let mut stream = connect(&bus).await;

        for _ in 0..1100 {
            bus.emit(CoreEvent::ProvidersChanged).await;
        }

        let closed = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match read_frame(&mut stream).await {
                    None => break,
                    Some((0x8, _)) => break,
                    Some(_) => {}
                }
            }
        })
        .await;
        assert!(closed.is_ok(), "socket stayed open after the client lagged");
    }

    #[tokio::test]
    async fn live_run_events_reach_clients_with_their_payloads() {
        let bus = Arc::new(WebSocketEventBus::new());
        let mut stream = connect(&bus).await;
        let run: pipedash_core::domain::PipelineRun = serde_json::from_value(serde_json::json!({
            "id": "p__3", "pipeline_id": "p", "run_number": 3, "status": "running",
            "started_at": "2026-01-01T00:00:00Z", "duration_seconds": 12, "logs_url": ""
        }))
        .unwrap();

        bus.emit(CoreEvent::RunLogTreeChanged {
            pipeline_id: "p".into(),
            run_number: 3,
        })
        .await;
        bus.emit(CoreEvent::RunUpdated {
            pipeline_id: "p".into(),
            run_number: 3,
            run: run.clone(),
        })
        .await;

        assert_eq!(
            next_message(&mut stream).await,
            serde_json::json!({
                "type": "run-log-tree-changed",
                "payload": { "pipelineId": "p", "runNumber": 3 },
            })
        );
        assert_eq!(
            next_message(&mut stream).await,
            serde_json::json!({
                "type": "run-updated",
                "payload": {
                    "pipelineId": "p",
                    "runNumber": 3,
                    "run": serde_json::to_value(&run).unwrap(),
                },
            })
        );
    }

    #[tokio::test]
    async fn a_browser_auth_frame_gets_auth_ok_when_no_token_is_required() {
        let bus = Arc::new(WebSocketEventBus::new());
        let mut stream = connect(&bus).await;

        send_json(&mut stream, json!({ "type": "auth", "token": "anything" })).await;

        assert_eq!(
            next_message(&mut stream).await,
            json!({
                "type": "auth-ok",
                "payload": { "host": "server", "name": null, "deviceId": null },
            })
        );
    }

    #[tokio::test]
    async fn the_operator_token_authenticates_and_requests_carry_it() {
        let state = AppState::setup_mode(Arc::new(WebSocketEventBus::new()));
        state.set_api_token("vault-password");

        let mut stream = connect_state(state.clone()).await;
        send_json(&mut stream, json!({ "type": "auth", "token": "wrong" })).await;
        let rejected = next_message(&mut stream).await;
        assert_eq!(rejected["type"], "auth-error");
        assert_eq!(rejected["payload"]["code"], "AUTH_FAILED");
        assert_closed(&mut stream).await;

        let mut stream = connect_state(state).await;
        send_json(
            &mut stream,
            json!({ "type": "auth", "token": "vault-password" }),
        )
        .await;
        assert_eq!(next_message(&mut stream).await["type"], "auth-ok");
        send_json(
            &mut stream,
            json!({ "type": "request", "id": 5, "method": "GET", "path": "/providers" }),
        )
        .await;
        let response = next_message(&mut stream).await;

        assert_eq!(response["type"], "response");
        assert_eq!(response["id"], 5);
        assert_eq!(response["status"], 503);
    }

    #[tokio::test]
    async fn a_request_before_authenticating_is_refused() {
        let state = AppState::setup_mode(Arc::new(WebSocketEventBus::new()));
        state.set_api_token("vault-password");
        let mut stream = connect_state(state).await;

        send_json(
            &mut stream,
            json!({ "type": "request", "id": 1, "method": "GET", "path": "/providers" }),
        )
        .await;

        assert_eq!(
            next_message(&mut stream).await["payload"]["code"],
            "AUTH_FAILED"
        );
        assert_closed(&mut stream).await;
    }

    #[tokio::test]
    async fn a_claim_pairs_the_device_and_serves_requests_then_the_token_reconnects() {
        let host = hosted("ws-claim").await;
        let (code, _) = host.state.pairing_codes.issue().unwrap();
        let mut stream = connect_state(host.state.clone()).await;

        let ok = claim(&mut stream, &code).await;

        assert_eq!(ok["type"], "auth-ok");
        assert_eq!(ok["payload"]["host"], "desktop");
        assert_eq!(ok["payload"]["name"], "test-mac");
        let device_id = ok["payload"]["deviceId"].as_str().unwrap().to_string();
        let token = ok["payload"]["token"].as_str().unwrap().to_string();
        assert_eq!(token.len(), 64);

        send_json(
            &mut stream,
            json!({ "type": "request", "id": 1, "method": "GET", "path": "/devices" }),
        )
        .await;
        let response = next_message(&mut stream).await;
        assert_eq!(response["id"], 1);
        assert_eq!(response["status"], 200);
        let body: Value = serde_json::from_str(response["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["devices"][0]["id"], device_id);
        assert_eq!(body["devices"][0]["current"], true);

        let mut again = connect_state(host.state.clone()).await;
        send_json(&mut again, json!({ "type": "auth", "token": token })).await;
        let ok = next_message(&mut again).await;
        assert_eq!(ok["payload"]["deviceId"], device_id);
        assert!(ok["payload"].get("token").is_none());

        let mut stranger = connect_state(host.state.clone()).await;
        send_json(&mut stranger, json!({ "type": "auth", "token": "unknown" })).await;
        assert_eq!(
            next_message(&mut stranger).await["payload"]["code"],
            "AUTH_FAILED"
        );
    }

    #[tokio::test]
    async fn stopping_the_host_closes_connected_sockets() {
        let state = AppState::hosted(Arc::new(WebSocketEventBus::new()), None, "test-mac");
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(crate::serve_host(listener, state, async move {
            let _ = stopped.await;
        }));
        let mut stream = upgrade(addr, "/api/v1/ws").await;

        stop.send(()).unwrap();

        assert_closed(&mut stream).await;
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .expect("serve_host kept running")
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn a_device_token_on_a_server_without_an_api_token_becomes_a_device() {
        let host = crate::test_support::server("ws-open-device").await;
        let core = host.state.core().await.unwrap();
        let (device, token) = core.pair_device("Pixel", "android").await.unwrap();
        let mut stream = connect_state(host.state.clone()).await;

        send_json(&mut stream, json!({ "type": "auth", "token": token })).await;
        let ok = next_message(&mut stream).await;
        assert_eq!(ok["payload"]["deviceId"], device.id);

        send_json(
            &mut stream,
            json!({ "type": "request", "id": 1, "method": "GET", "path": "/devices" }),
        )
        .await;
        let response = next_message(&mut stream).await;
        let body: Value = serde_json::from_str(response["body"].as_str().unwrap()).unwrap();
        assert_eq!(body["devices"][0]["current"], true);

        core.revoke_device(&device.id).await.unwrap();
        let removed = next_message(&mut stream).await;
        assert_eq!(removed["payload"]["code"], "DEVICE_REMOVED");
        assert_closed(&mut stream).await;
    }

    #[tokio::test]
    async fn a_failed_device_lookup_is_not_reported_as_a_revoked_device() {
        let host = hosted("ws-lookup-failure").await;
        let core = host.state.core().await.unwrap();
        let (_, token) = core.pair_device("Pixel", "android").await.unwrap();
        let pipedash_core::infrastructure::database::DatabasePool::Sqlite(pool) =
            core.provider_service.repository().cache_pool().clone()
        else {
            panic!("expected a SQLite pool");
        };
        pool.close().await;
        let mut stream = connect_state(host.state.clone()).await;

        send_json(&mut stream, json!({ "type": "auth", "token": token })).await;

        let rejected = next_message(&mut stream).await;
        assert_eq!(rejected["payload"]["code"], "SERVER_NOT_READY");
        assert_closed(&mut stream).await;
    }

    #[tokio::test]
    async fn a_locked_host_answers_host_locked_instead_of_forgetting_the_device() {
        let state = AppState::hosted(Arc::new(WebSocketEventBus::new()), None, "test-mac");
        let mut stream = connect_state(state).await;

        send_json(&mut stream, json!({ "type": "auth", "token": "any" })).await;

        let rejected = next_message(&mut stream).await;
        assert_eq!(rejected["payload"]["code"], "HOST_LOCKED");
        assert_eq!(rejected["payload"]["message"], crate::HOST_LOCKED_MESSAGE);
        assert_closed(&mut stream).await;
    }

    #[tokio::test]
    async fn a_pairing_code_can_be_claimed_only_once() {
        let host = hosted("ws-claim-once").await;
        let (code, _) = host.state.pairing_codes.issue().unwrap();

        let mut first = connect_state(host.state.clone()).await;
        assert_eq!(claim(&mut first, &code).await["type"], "auth-ok");

        let mut second = connect_state(host.state.clone()).await;
        let rejected = claim(&mut second, &code).await;
        assert_eq!(rejected["type"], "auth-error");
        assert_eq!(rejected["payload"]["code"], "PAIRING_CODE_INVALID");
        assert_closed(&mut second).await;
    }

    #[tokio::test]
    async fn a_claim_with_invalid_device_details_is_rejected_without_burning_the_code() {
        let host = hosted("ws-claim-invalid").await;
        let (code, _) = host.state.pairing_codes.issue().unwrap();
        let mut stream = connect_state(host.state.clone()).await;

        send_json(
            &mut stream,
            json!({ "type": "claim", "code": code, "name": "Pixel", "platform": "palm" }),
        )
        .await;

        assert_eq!(
            next_message(&mut stream).await["payload"]["code"],
            "INVALID_PLATFORM"
        );
        assert!(host.state.pairing_codes.take(&code));
    }

    #[tokio::test]
    async fn revoking_a_device_closes_its_socket_with_device_removed() {
        let host = hosted("ws-revoke").await;
        let (code, _) = host.state.pairing_codes.issue().unwrap();
        let mut stream = connect_state(host.state.clone()).await;
        let ok = claim(&mut stream, &code).await;
        let device_id = ok["payload"]["deviceId"].as_str().unwrap().to_string();

        host.state
            .core()
            .await
            .unwrap()
            .revoke_device(&device_id)
            .await
            .unwrap();

        let removed = next_message(&mut stream).await;
        assert_eq!(removed["type"], "auth-error");
        assert_eq!(removed["payload"]["code"], "DEVICE_REMOVED");
        assert_closed(&mut stream).await;
    }

    #[tokio::test]
    async fn a_connection_that_never_authenticates_is_closed_after_ten_seconds() {
        let host = hosted("ws-timeout").await;
        let mut stream = connect_state(host.state.clone()).await;

        tokio::time::pause();
        tokio::time::advance(AUTH_TIMEOUT + Duration::from_secs(1)).await;

        let timed_out = next_message(&mut stream).await;
        assert_eq!(timed_out["type"], "auth-error");
        assert_eq!(timed_out["payload"]["code"], "AUTH_TIMEOUT");
        assert_closed(&mut stream).await;
    }
}
