use axum::extract::ws::Message;
use pipedash_core::domain::pairing::validate_device;
use serde_json::json;

use super::connection::Connection;
use super::protocol::{
    text,
    Identity,
    AUTH_FAILED_MESSAGE,
};
use crate::state::AppState;

async fn touch_device(state: &AppState, id: &str) {
    let Ok(core) = state.core().await else {
        return;
    };
    if let Err(error) = core.devices().touch(id).await {
        tracing::warn!(%error, "Failed to record device activity");
    }
}

impl Connection {
    pub(super) async fn reject(&self, code: &str, message: &str) {
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

    async fn reject_unauthenticated(&self) {
        let config_error = self.state.inner.read().await.config_error.is_some();
        let no_core = self.state.core().await.is_err();
        if no_core && self.state.desktop_host.is_some() {
            self.reject("HOST_LOCKED", crate::HOST_LOCKED_MESSAGE).await;
        } else if no_core && config_error {
            self.reject("SERVER_NOT_READY", crate::SERVER_NOT_READY_MESSAGE)
                .await;
        } else {
            self.reject("AUTH_FAILED", AUTH_FAILED_MESSAGE).await;
        }
    }

    pub(super) async fn on_auth(&mut self, token: &str) -> bool {
        if self.events.is_none() {
            self.events = Some(self.state.ws_event_bus.subscribe());
        }
        let anonymous = matches!(self.identity, Some(Identity::Anonymous));

        if self
            .state
            .api_token()
            .is_some_and(|expected| crate::token_matches(token, &expected))
        {
            return self
                .authenticated(Identity::Operator(token.to_string()), None)
                .await;
        }

        let identity = match self.state.paired_device(token).await {
            Ok(Some(device)) => {
                touch_device(&self.state, &device.id).await;
                Identity::Device {
                    id: device.id,
                    token: token.to_string(),
                }
            }
            Ok(None) if anonymous => Identity::Anonymous,
            Ok(None) => {
                self.reject_unauthenticated().await;
                return false;
            }
            Err(_) => {
                self.reject("SERVER_NOT_READY", crate::SERVER_NOT_READY_MESSAGE)
                    .await;
                return false;
            }
        };

        self.authenticated(identity, None).await
    }

    pub(super) async fn on_claim(&mut self, code: &str, name: &str, platform: &str) -> bool {
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
            Ok(_) if crate::demo::enabled() => crate::demo::device_name(platform),
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
}
