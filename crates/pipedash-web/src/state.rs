use std::sync::Arc;

use pipedash_core::infrastructure::StorageManager;
use pipedash_core::CoreContext;
use tokio::sync::RwLock;

use crate::ws::WebSocketEventBus;

pub struct AppStateInner {
    pub core: Option<CoreContext>,
    pub storage_manager: Option<StorageManager>,
    pub setup_required: bool,
    pub config_error: Option<String>,
    pub token_store_ready: bool,
}

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<RwLock<AppStateInner>>,
    pub ws_event_bus: Arc<WebSocketEventBus>,
}

impl AppState {
    pub fn setup_mode(ws_event_bus: Arc<WebSocketEventBus>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(AppStateInner {
                core: None,
                storage_manager: None,
                setup_required: true,
                config_error: None,
                token_store_ready: false,
            })),
            ws_event_bus,
        }
    }

    pub fn config_error(ws_event_bus: Arc<WebSocketEventBus>, error: String) -> Self {
        Self {
            inner: Arc::new(RwLock::new(AppStateInner {
                core: None,
                storage_manager: None,
                setup_required: false,
                config_error: Some(error),
                token_store_ready: false,
            })),
            ws_event_bus,
        }
    }

    pub fn initialized(
        ws_event_bus: Arc<WebSocketEventBus>, core: CoreContext, storage_manager: StorageManager,
    ) -> Self {
        Self {
            inner: Arc::new(RwLock::new(AppStateInner {
                core: Some(core),
                storage_manager: Some(storage_manager),
                setup_required: false,
                config_error: None,
                token_store_ready: false,
            })),
            ws_event_bus,
        }
    }
}
