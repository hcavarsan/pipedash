use std::sync::Arc;

use async_trait::async_trait;
use pipedash_core::application::ShownNotification;
use pipedash_core::event::{
    CoreEvent,
    EventBus,
};
use serde_json::Value;
use tauri::{
    AppHandle,
    Emitter,
    Manager,
    Runtime,
};

use crate::local_notifications::LocalNotificationsState;

pub(crate) const NOTIFICATION_EVENT: &str = "notification";

pub struct TauriEventBus<R: Runtime> {
    app_handle: AppHandle<R>,
    #[cfg_attr(mobile, allow(dead_code))]
    generation: u64,
}

impl<R: Runtime> TauriEventBus<R> {
    pub fn new(app_handle: AppHandle<R>, generation: u64) -> Self {
        Self {
            app_handle,
            generation,
        }
    }

    fn admit(&self, event: &CoreEvent) -> Option<ShownNotification> {
        let state = self.app_handle.try_state::<LocalNotificationsState>()?;

        admit(&state, event)
    }
}

fn admit(state: &LocalNotificationsState, event: &CoreEvent) -> Option<ShownNotification> {
    match event {
        CoreEvent::NotificationObserved { notification } => state.admit(notification),
        _ => None,
    }
}

fn webview_payload(
    event: &CoreEvent, shown: Option<&ShownNotification>,
) -> Option<(&'static str, Value)> {
    match event {
        CoreEvent::NotificationObserved { .. } => {
            Some((NOTIFICATION_EVENT, serde_json::to_value(shown?).ok()?))
        }
        _ => Some((event.event_name(), event.to_json_payload())),
    }
}

unsafe impl<R: Runtime> Send for TauriEventBus<R> {}
unsafe impl<R: Runtime> Sync for TauriEventBus<R> {}

#[async_trait]
impl<R: Runtime + 'static> EventBus for TauriEventBus<R> {
    async fn emit(&self, event: CoreEvent) {
        let shown = self.admit(&event);
        #[cfg(desktop)]
        {
            crate::desktop::projection::observe(&self.app_handle, &event);
            if let Some(shown) = &shown {
                crate::desktop::notifications::record_shown(
                    &self.app_handle,
                    self.generation,
                    shown,
                );
            }
            if let CoreEvent::ProviderRemoved { provider, .. } = &event {
                crate::build_watch::interrupt_provider_generation(
                    &self.app_handle,
                    self.generation,
                    provider.id,
                );
            }
        }
        #[cfg(mobile)]
        {
            crate::local_notifications::events::on_event(&self.app_handle, &event).await;
            if let CoreEvent::ProviderRemoved { provider, .. } = &event {
                crate::build_watch::interrupt_provider(&self.app_handle, provider.id);
            }
        }
        if let Some((event_name, payload)) = webview_payload(&event, shown.as_ref()) {
            if let Err(e) = self.app_handle.emit(event_name, payload) {
                tracing::error!("Failed to emit event '{}': {}", event_name, e);
            }
        }
        #[cfg(desktop)]
        if let Some(host) = self
            .app_handle
            .try_state::<crate::host::desktop::HostEvents>()
        {
            let _ = host.0.send(event);
        }
    }

    async fn emit_to(&self, target: &str, event: CoreEvent) {
        let shown = self.admit(&event);
        let Some((event_name, payload)) = webview_payload(&event, shown.as_ref()) else {
            return;
        };
        if let Err(e) = self.app_handle.emit_to(target, event_name, payload) {
            tracing::error!(
                "Failed to emit event '{}' to '{}': {}",
                event_name,
                target,
                e
            );
        }
    }
}

pub fn create_tauri_event_bus<R: Runtime + 'static>(
    app_handle: AppHandle<R>, generation: u64,
) -> Arc<dyn EventBus> {
    Arc::new(TauriEventBus::new(app_handle, generation))
}

#[cfg(test)]
mod tests;
