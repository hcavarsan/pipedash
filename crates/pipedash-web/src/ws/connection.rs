use std::sync::Arc;

use axum::{
    extract::ws::Message,
    Router,
};
use pipedash_core::application::ObservedNotification;
use pipedash_core::domain::NotificationPreferences;
use pipedash_core::event::CoreEvent;
use pipedash_core::CoreContext;
use serde_json::json;
use tokio::sync::{
    broadcast,
    mpsc,
    Semaphore,
};
use tokio::task::JoinSet;

use super::dispatch::dispatch;
use super::protocol::{
    event_frame,
    response_frame,
    text,
    ClientFrame,
    Identity,
    AUTH_FAILED_MESSAGE,
    TOO_MANY_REQUESTS,
};
use crate::state::AppState;

const MAX_IN_FLIGHT: usize = 16;
pub(super) const MAX_ADMITTED: usize = 64;

pub(super) struct Connection {
    pub(super) state: AppState,
    pub(super) out: mpsc::Sender<Message>,
    pub(super) identity: Option<Identity>,
    pub(super) events: Option<broadcast::Receiver<CoreEvent>>,
    pub(super) router: Option<Router>,
    permits: Arc<Semaphore>,
    admitted: Arc<Semaphore>,
    pub(super) requests: JoinSet<()>,
    preferences: NotificationPreferences,
    last_notification: u64,
}

impl Connection {
    pub(super) fn new(state: AppState, out: mpsc::Sender<Message>) -> Self {
        let identity = (!state.requires_auth()).then_some(Identity::Anonymous);
        let events = identity.as_ref().map(|_| state.ws_event_bus.subscribe());

        Self {
            state,
            out,
            identity,
            events,
            router: None,
            permits: Arc::new(Semaphore::new(MAX_IN_FLIGHT)),
            admitted: Arc::new(Semaphore::new(MAX_ADMITTED)),
            requests: JoinSet::new(),
            preferences: NotificationPreferences::default(),
            last_notification: 0,
        }
    }

    pub(super) async fn on_text(&mut self, raw: &str) -> bool {
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
            ClientFrame::NotificationPreferences { preferences, after } => {
                self.on_notification_preferences(preferences, after).await
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
        let Ok(admitted) = Arc::clone(&self.admitted).try_acquire_owned() else {
            let _ = self
                .out
                .try_send(response_frame(id, 429, TOO_MANY_REQUESTS.as_bytes()));
            return true;
        };
        let permits = Arc::clone(&self.permits);
        let out = self.out.clone();
        self.requests.spawn(async move {
            let _admitted = admitted;
            let Ok(_permit) = permits.acquire_owned().await else {
                return;
            };
            let (status, body) = dispatch(router, &method, &path, body, token.as_deref()).await;
            let _ = out.send(response_frame(id, status, &body)).await;
        });

        true
    }

    async fn on_notification_preferences(
        &mut self, preferences: NotificationPreferences, after: Option<u64>,
    ) -> bool {
        match self.identity {
            None => {
                self.reject("AUTH_FAILED", AUTH_FAILED_MESSAGE).await;
                return false;
            }
            Some(Identity::Device { .. }) => return true,
            Some(_) => {}
        }
        self.preferences.merge(&preferences);

        let Some(after) = after else {
            return true;
        };
        let Ok(core) = self.state.core().await else {
            return true;
        };
        for observed in core.notifications_after(after) {
            if !self.show(&core, &observed).await {
                return false;
            }
        }

        true
    }

    async fn on_notification(&mut self, observed: &ObservedNotification) -> bool {
        match self.state.core().await {
            Ok(core) => self.show(&core, observed).await,
            Err(_) => true,
        }
    }

    async fn show(&mut self, core: &CoreContext, observed: &ObservedNotification) -> bool {
        if observed.id <= self.last_notification {
            return true;
        }
        let shown = match &self.identity {
            None => return true,
            Some(Identity::Device { id, .. }) => {
                let mut preferences = match core.devices().notification_preferences().await {
                    Ok(mut stored) => stored.remove(id).unwrap_or_default(),
                    Err(error) => {
                        tracing::warn!(%error, "Couldn't load device notification preferences");
                        return true;
                    }
                };
                observed.admit(&mut preferences)
            }
            Some(_) => observed.admit(&mut self.preferences),
        };
        let Some(shown) = shown else {
            return true;
        };
        self.last_notification = observed.id;

        self.out
            .send(text(json!({ "type": "notification", "payload": shown })))
            .await
            .is_ok()
    }

    pub(super) async fn on_event(
        &mut self, event: Result<CoreEvent, broadcast::error::RecvError>,
    ) -> bool {
        match event {
            Ok(CoreEvent::NotificationObserved { notification }) => {
                self.on_notification(&notification).await
            }
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
