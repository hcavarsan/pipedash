use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use serde::Deserialize;
use serde_json::{
    json,
    Value,
};
use tokio_tungstenite::tungstenite::Message;

use crate::remote::store::HostKind;

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
    pub(super) fn frame(&self) -> String {
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

pub(super) fn request_frame(id: u64, method: &str, path: &str, body: Option<&str>) -> Message {
    Message::Text(
        json!({ "type": "request", "id": id, "method": method, "path": path, "body": body })
            .to_string()
            .into(),
    )
}

pub(super) fn revokes_access(code: &str) -> bool {
    matches!(code, "AUTH_FAILED" | "DEVICE_REMOVED")
}
