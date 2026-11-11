use std::time::Duration;

use axum::{
    body::{
        to_bytes,
        Body,
        Bytes,
    },
    extract::ws::{
        Message,
        WebSocket,
    },
    http::{
        header::{
            AUTHORIZATION,
            CONTENT_TYPE,
        },
        Request,
    },
    Router,
};
use futures_util::{
    stream::SplitSink,
    SinkExt,
};
use tokio::sync::mpsc;
use tower::ServiceExt;

use super::protocol::{
    INVALID_PATH,
    RESPONSE_TOO_LARGE,
};

const PING_INTERVAL: Duration = Duration::from_secs(20);
const WRITE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;

pub(super) async fn dispatch(
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

pub(super) async fn write_loop(
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
