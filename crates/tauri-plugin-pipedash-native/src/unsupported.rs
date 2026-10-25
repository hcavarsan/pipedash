use std::{
    marker::PhantomData,
    path::Path,
};

use serde::de::DeserializeOwned;
use tauri::{
    plugin::PluginApi,
    AppHandle,
    Runtime,
};

use crate::{
    Accessibility,
    Appearance,
    Error,
    HapticKind,
    NotificationResponse,
    PrivacyCover,
    Result,
    ShareRect,
};

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>, _api: PluginApi<R, C>,
) -> Result<PipedashNative<R>> {
    Ok(PipedashNative(PhantomData))
}

pub struct PipedashNative<R: Runtime>(PhantomData<fn() -> R>);

impl<R: Runtime> PipedashNative<R> {
    pub async fn haptic(&self, _kind: HapticKind) -> Result<()> {
        Ok(())
    }

    pub async fn set_appearance(&self, _scheme: Appearance) -> Result<()> {
        Ok(())
    }

    pub async fn accessibility(&self) -> Result<Accessibility> {
        Ok(Accessibility::default())
    }

    pub async fn open_settings(&self) -> Result<()> {
        Err(Error::Unsupported)
    }

    pub async fn set_badge(&self, _count: u32) -> Result<()> {
        Ok(())
    }

    pub async fn configure_privacy_cover(&self, _cover: PrivacyCover) -> Result<()> {
        Ok(())
    }

    pub async fn release_privacy_cover(&self) -> Result<()> {
        Ok(())
    }

    pub async fn share_file(&self, _path: &Path) -> Result<bool> {
        Err(Error::Unsupported)
    }

    pub async fn share_link(&self, _url: &str, _source_rect: ShareRect) -> Result<bool> {
        Err(Error::Unsupported)
    }

    pub fn on_notification_response<F: Fn(NotificationResponse) + Send + Sync + 'static>(
        &self, _handler: F,
    ) -> Result<()> {
        Ok(())
    }

    pub fn on_status_bar_tap<F: Fn() + Send + Sync + 'static>(&self, _handler: F) -> Result<()> {
        Ok(())
    }
}
