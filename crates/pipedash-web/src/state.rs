use std::sync::{
    Arc,
    PoisonError,
    RwLock as StdRwLock,
};

use pipedash_core::application::{
    PipelineService,
    ProviderService,
};
use pipedash_core::infrastructure::{
    ConfigState,
    StorageManager,
};
use pipedash_core::CoreContext;
use tokio::sync::{
    Mutex,
    RwLock,
};

use crate::error::{
    ApiResult,
    AppError,
};
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
    pub setup_lock: Arc<Mutex<()>>,
    api_token: Arc<StdRwLock<Option<Arc<str>>>>,
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
            setup_lock: Arc::default(),
            api_token: Arc::default(),
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
            setup_lock: Arc::default(),
            api_token: Arc::default(),
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
            setup_lock: Arc::default(),
            api_token: Arc::default(),
        }
    }

    pub fn api_token(&self) -> Option<Arc<str>> {
        self.api_token
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn set_api_token(&self, token: &str) {
        *self
            .api_token
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(token.into());
    }

    pub async fn provider_service(&self) -> ApiResult<Arc<ProviderService>> {
        let inner = self.inner.read().await;
        let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
        Ok(Arc::clone(&core.provider_service))
    }

    pub async fn pipeline_service(&self) -> ApiResult<Arc<PipelineService>> {
        let inner = self.inner.read().await;
        let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
        Ok(Arc::clone(&core.pipeline_service))
    }

    pub async fn config_state(&self) -> ApiResult<Arc<ConfigState>> {
        let inner = self.inner.read().await;
        let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
        Ok(Arc::clone(&core.config_state))
    }
}
