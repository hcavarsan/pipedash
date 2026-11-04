use std::sync::{
    Arc,
    PoisonError,
    RwLock as StdRwLock,
};

use pipedash_core::application::{
    LogService,
    PipelineService,
    PipelineViewService,
    ProviderService,
};
use pipedash_core::domain::{
    DomainError,
    PairedDevice,
};
use pipedash_core::infrastructure::{
    ConfigKey,
    ConfigState,
    StorageManager,
};
use pipedash_core::CoreContext;
use tokio::sync::{
    watch,
    Mutex,
    RwLock,
};

use crate::download_tickets::DownloadTickets;
use crate::error::{
    ApiResult,
    AppError,
};
use crate::pairing_codes::PairingCodes;
use crate::push::normalize_relay_url;
use crate::ws::WebSocketEventBus;

pub struct AppStateInner {
    pub core: Option<CoreContext>,
    pub storage_manager: Option<StorageManager>,
    pub setup_required: bool,
    pub config_error: Option<String>,
    pub token_store_ready: bool,
}

#[derive(Clone)]
pub struct CurrentDevice(pub String);

#[derive(Clone)]
pub struct AppState {
    pub inner: Arc<RwLock<AppStateInner>>,
    pub ws_event_bus: Arc<WebSocketEventBus>,
    pub setup_lock: Arc<Mutex<()>>,
    pub download_tickets: Arc<DownloadTickets>,
    pub pairing_codes: Arc<PairingCodes>,
    pub desktop_host: Option<Arc<str>>,
    pub push_relay: Option<Arc<str>>,
    pub closing: Arc<watch::Sender<bool>>,
    api_token: Arc<StdRwLock<Option<Arc<str>>>>,
}

impl AppState {
    fn new(
        ws_event_bus: Arc<WebSocketEventBus>, inner: AppStateInner, desktop_host: Option<Arc<str>>,
    ) -> Self {
        Self {
            inner: Arc::new(RwLock::new(inner)),
            ws_event_bus,
            setup_lock: Arc::default(),
            download_tickets: Arc::default(),
            pairing_codes: Arc::default(),
            desktop_host,
            push_relay: None,
            closing: Arc::new(watch::channel(false).0),
            api_token: Arc::default(),
        }
    }

    pub fn setup_mode(ws_event_bus: Arc<WebSocketEventBus>) -> Self {
        Self::new(
            ws_event_bus,
            AppStateInner {
                core: None,
                storage_manager: None,
                setup_required: true,
                config_error: None,
                token_store_ready: false,
            },
            None,
        )
    }

    pub fn config_error(ws_event_bus: Arc<WebSocketEventBus>, error: String) -> Self {
        Self::new(
            ws_event_bus,
            AppStateInner {
                core: None,
                storage_manager: None,
                setup_required: false,
                config_error: Some(error),
                token_store_ready: false,
            },
            None,
        )
    }

    pub fn initialized(
        ws_event_bus: Arc<WebSocketEventBus>, core: CoreContext, storage_manager: StorageManager,
    ) -> Self {
        Self::new(
            ws_event_bus,
            AppStateInner {
                core: Some(core),
                storage_manager: Some(storage_manager),
                setup_required: false,
                config_error: None,
                token_store_ready: false,
            },
            None,
        )
    }

    pub fn hosted(
        ws_event_bus: Arc<WebSocketEventBus>, core: Option<CoreContext>, name: &str,
    ) -> Self {
        Self::new(
            ws_event_bus,
            AppStateInner {
                token_store_ready: core.is_some(),
                core,
                storage_manager: None,
                setup_required: false,
                config_error: None,
            },
            Some(name.into()),
        )
    }

    pub async fn set_core(&self, core: Option<CoreContext>) {
        let mut inner = self.inner.write().await;
        inner.token_store_ready = core.is_some();
        inner.core = core;
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

    pub fn requires_auth(&self) -> bool {
        self.api_token().is_some() || self.desktop_host.is_some()
    }

    pub async fn paired_device(&self, token: &str) -> Result<Option<PairedDevice>, DomainError> {
        let Some(core) = self.inner.read().await.core.clone() else {
            return Ok(None);
        };

        core.devices()
            .find_by_token(token)
            .await
            .inspect_err(|error| tracing::warn!(%error, "Failed to look up the paired device"))
    }

    pub fn with_push_relay(mut self, url: &str) -> Self {
        self.push_relay = Some(url.into());
        self
    }

    pub async fn push_relay_url(&self) -> Option<String> {
        if self.desktop_host.is_some() {
            return self.push_relay.as_deref().and_then(normalize_relay_url);
        }

        let config = self.config_state().await.ok()?;

        normalize_relay_url(&config.get_string(ConfigKey::PushRelayUrl).await)
    }

    pub async fn core(&self) -> ApiResult<CoreContext> {
        self.inner
            .read()
            .await
            .core
            .clone()
            .ok_or_else(AppError::not_initialized)
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

    pub async fn pipeline_view_service(&self) -> ApiResult<Arc<PipelineViewService>> {
        let inner = self.inner.read().await;
        let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
        Ok(Arc::clone(&core.pipeline_view_service))
    }

    pub async fn log_service(&self) -> ApiResult<Arc<LogService>> {
        let inner = self.inner.read().await;
        let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
        Ok(Arc::clone(&core.log_service))
    }

    pub async fn config_state(&self) -> ApiResult<Arc<ConfigState>> {
        let inner = self.inner.read().await;
        let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
        Ok(Arc::clone(&core.config_state))
    }
}
