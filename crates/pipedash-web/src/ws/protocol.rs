use axum::extract::ws::Message;
use base64::{
    engine::general_purpose::STANDARD,
    Engine,
};
use pipedash_core::domain::NotificationPreferences;
use pipedash_core::event::CoreEvent;
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::json;

pub(super) const AUTH_FAILED_MESSAGE: &str = "This device isn't paired with this Pipedash.";
pub(super) const RESPONSE_TOO_LARGE: &str = r#"{"code":"RESPONSE_TOO_LARGE","message":"The response is too large to send to this device."}"#;
pub(super) const INVALID_PATH: &str = r#"{"code":"BAD_REQUEST","message":"Invalid request path."}"#;
pub(super) const TOO_MANY_REQUESTS: &str = r#"{"code":"TOO_MANY_REQUESTS","message":"This device sent too many requests at once. Try again."}"#;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub(super) enum ClientFrame {
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
    NotificationPreferences {
        preferences: NotificationPreferences,
        #[serde(default)]
        after: Option<u64>,
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

pub(super) enum Identity {
    Anonymous,
    Operator(String),
    Device { id: String, token: String },
}

pub(super) fn event_frame(event: &CoreEvent) -> Option<String> {
    serde_json::to_string(&json!({
        "type": event.event_name(),
        "payload": event.to_json_payload(),
    }))
    .ok()
}

pub(super) fn text(value: serde_json::Value) -> Message {
    Message::Text(value.to_string().into())
}

pub(super) fn response_frame(id: u64, status: u16, body: &[u8]) -> Message {
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
