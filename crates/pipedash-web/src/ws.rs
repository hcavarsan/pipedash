use async_trait::async_trait;
use axum::{
    extract::{
        ws::{
            Message,
            WebSocket,
            WebSocketUpgrade,
        },
        State,
    },
    response::IntoResponse,
};
use futures_util::{
    SinkExt,
    StreamExt,
};
use pipedash_core::event::{
    CoreEvent,
    EventBus,
};
use serde::Deserialize;
use tokio::sync::broadcast;

use crate::state::AppState;

#[derive(Deserialize)]
struct AuthMessage {
    #[serde(rename = "type")]
    msg_type: String,
    token: String,
}

pub struct WebSocketEventBus {
    tx: broadcast::Sender<CoreEvent>,
}

impl WebSocketEventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
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

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    if let Some(expected_token) = state.api_token() {
        match receiver.next().await {
            Some(Ok(Message::Text(text))) => {
                if let Ok(auth) = serde_json::from_str::<AuthMessage>(&text) {
                    if auth.msg_type != "auth"
                        || !crate::token_matches(&auth.token, &expected_token)
                    {
                        let _ = sender.close().await;
                        return;
                    }
                } else {
                    let _ = sender.close().await;
                    return;
                }
            }
            _ => {
                let _ = sender.close().await;
                return;
            }
        }
    }

    let mut rx = state.ws_event_bus.subscribe();

    let mut send_task = tokio::spawn(async move {
        loop {
            let event = match rx.recv().await {
                Ok(event) => event,
                Err(broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(skipped, "WebSocket client fell behind, closing");
                    let _ = sender.close().await;
                    break;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            };

            let event_data = serde_json::json!({
                "type": event.event_name(),
                "payload": event.to_json_payload(),
            });

            if let Ok(json) = serde_json::to_string(&event_data) {
                if sender.send(Message::Text(json.into())).await.is_err() {
                    break;
                }
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Ping(data)) => {
                    tracing::trace!("Received ping: {:?}", data);
                }
                Ok(Message::Close(_)) => {
                    tracing::debug!("WebSocket client disconnected");
                    break;
                }
                Ok(Message::Text(text)) => {
                    tracing::trace!("Received text: {}", text);
                }
                Err(e) => {
                    tracing::warn!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use axum::routing::get;
    use axum::Router;
    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };
    use tokio::net::{
        TcpListener,
        TcpStream,
    };

    use super::*;

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

    async fn connect(bus: &Arc<WebSocketEventBus>) -> TcpStream {
        let app = Router::new()
            .route("/ws", get(ws_handler))
            .with_state(AppState::setup_mode(bus.clone()));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let mut stream = TcpStream::connect(addr).await.unwrap();
        let handshake = format!(
            "GET /ws HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
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

    async fn next_message(stream: &mut TcpStream) -> serde_json::Value {
        let (opcode, payload) = tokio::time::timeout(Duration::from_secs(5), read_frame(stream))
            .await
            .expect("no message arrived")
            .expect("the socket closed");
        assert_eq!(opcode, 0x1);
        serde_json::from_slice(&payload).unwrap()
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
}
