use std::sync::Arc;

use async_trait::async_trait;
use pipedash_core::event::{
    CoreEvent,
    EventBus,
};
use tauri::{
    AppHandle,
    Emitter,
    Runtime,
};

pub struct TauriEventBus<R: Runtime> {
    app_handle: AppHandle<R>,
}

impl<R: Runtime> TauriEventBus<R> {
    pub fn new(app_handle: AppHandle<R>) -> Self {
        Self { app_handle }
    }
}

unsafe impl<R: Runtime> Send for TauriEventBus<R> {}
unsafe impl<R: Runtime> Sync for TauriEventBus<R> {}

#[async_trait]
impl<R: Runtime + 'static> EventBus for TauriEventBus<R> {
    async fn emit(&self, event: CoreEvent) {
        let event_name = event.event_name();
        if let Err(e) = self.app_handle.emit(event_name, event.to_json_payload()) {
            tracing::error!("Failed to emit event '{}': {}", event_name, e);
        }
    }

    async fn emit_to(&self, target: &str, event: CoreEvent) {
        let event_name = event.event_name();
        if let Err(e) = self
            .app_handle
            .emit_to(target, event_name, event.to_json_payload())
        {
            tracing::error!(
                "Failed to emit event '{}' to '{}': {}",
                event_name,
                target,
                e
            );
        }
    }
}

pub fn create_tauri_event_bus<R: Runtime + 'static>(app_handle: AppHandle<R>) -> Arc<dyn EventBus> {
    Arc::new(TauriEventBus::new(app_handle))
}
