use std::marker::PhantomData;

use serde::de::DeserializeOwned;
use tauri::{
    plugin::PluginApi,
    AppHandle,
    Runtime,
};

use crate::{
    Error,
    Result,
};

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>, _api: PluginApi<R, C>,
) -> Result<PipedashPush<R>> {
    Ok(PipedashPush(PhantomData))
}

pub struct PipedashPush<R: Runtime>(PhantomData<fn() -> R>);

impl<R: Runtime> PipedashPush<R> {
    pub async fn is_enabled(&self) -> Result<bool> {
        Ok(false)
    }

    pub fn register_for_remote_notifications(&self) -> Result<String> {
        Err(Error::Unsupported)
    }
}
