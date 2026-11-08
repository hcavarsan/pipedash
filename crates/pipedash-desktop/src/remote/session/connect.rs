use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures::{
    SinkExt,
    StreamExt,
};
use pipedash_core::domain::pairing::is_private_ip;
use pipedash_core::domain::ServerUrl;
use rustls::ClientConfig;
use rustls_platform_verifier::ConfigVerifierExt;
use tokio::net::{
    lookup_host,
    TcpStream,
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

use super::protocol::{
    parse_server_frame,
    request_frame,
    AuthOk,
    ClientHello,
    ServerFrame,
};
use crate::remote::model::RemoteError;

pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const REVOKE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_MESSAGE_BYTES: usize = 128 * 1024 * 1024;

pub type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

fn socket_config() -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(MAX_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_MESSAGE_BYTES))
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

pub(super) fn require_private(host: &str, addrs: &[SocketAddr]) -> Result<(), RemoteError> {
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

async fn connect_and_greet(
    url: &ServerUrl, hello: &ClientHello<'_>,
) -> Result<WsStream, RemoteError> {
    let mut stream = connect(url).await?;
    stream
        .send(Message::Text(hello.frame().into()))
        .await
        .map_err(|_| RemoteError::Unreachable(url.host.clone()))?;

    Ok(stream)
}

pub(crate) async fn open(
    url: &ServerUrl, hello: ClientHello<'_>,
) -> Result<(WsStream, AuthOk), RemoteError> {
    let mut stream = tokio::time::timeout(CONNECT_TIMEOUT, connect_and_greet(url, &hello))
        .await
        .map_err(|_| RemoteError::Unreachable(url.host.clone()))??;
    let ok = tokio::time::timeout(AUTH_TIMEOUT, await_auth(&mut stream, &url.host))
        .await
        .map_err(|_| RemoteError::Timeout)??;

    Ok((stream, ok))
}

pub(crate) async fn request_once(
    stream: &mut WsStream, method: &str, path: &str, body: Option<&str>,
) -> Option<(u16, Vec<u8>)> {
    stream
        .send(request_frame(1, method, path, body))
        .await
        .ok()?;
    while let Some(Ok(message)) = stream.next().await {
        if let Message::Text(text) = message {
            match parse_server_frame(&text) {
                Some(ServerFrame::Response { status, body, .. }) => return Some((status, body)),
                Some(ServerFrame::AuthError { .. }) => return None,
                _ => {}
            }
        }
    }

    None
}

pub(crate) async fn revoke(mut stream: WsStream, device_id: &str) {
    let path = format!("/devices/{device_id}");
    let _ = tokio::time::timeout(REVOKE_TIMEOUT, async {
        request_once(&mut stream, "DELETE", &path, None).await;
        let _ = stream.close(None).await;
    })
    .await;
}
