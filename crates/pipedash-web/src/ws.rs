mod auth;
mod connection;
mod dispatch;
pub mod event_bus;
mod protocol;

use std::time::Duration;

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
use futures_util::StreamExt;
use pipedash_core::event::CoreEvent;
use tokio::sync::{
    broadcast,
    mpsc,
    watch,
};

use self::connection::Connection;
use self::dispatch::write_loop;
use crate::state::AppState;

const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const OUTBOX_CAPACITY: usize = 64;

async fn next_event(
    events: &mut Option<broadcast::Receiver<CoreEvent>>,
) -> Result<CoreEvent, broadcast::error::RecvError> {
    match events {
        Some(events) => events.recv().await,
        None => std::future::pending().await,
    }
}

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

pub(crate) async fn closed(closing: &mut watch::Receiver<bool>) {
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
mod tests;
