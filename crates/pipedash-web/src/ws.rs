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

/// Get the current WebSocket auth token from environment variable.
/// This is read dynamically to support vault unlock/lock operations.
fn get_ws_auth_token() -> Option<String> {
    std::env::var("PIPEDASH_VAULT_PASSWORD").ok()
}

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

    if let Some(expected_token) = get_ws_auth_token() {
        match receiver.next().await {
            Some(Ok(Message::Text(text))) => {
                if let Ok(auth) = serde_json::from_str::<AuthMessage>(&text) {
                    if auth.msg_type != "auth" || auth.token != expected_token {
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

    let send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
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

    send_task.abort();
}
