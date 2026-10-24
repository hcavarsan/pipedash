use serde::{
    de::DeserializeOwned,
    Deserialize,
};
use tauri::{
    plugin::{
        mobile::PluginInvokeError,
        PluginApi,
        PluginHandle,
    },
    AppHandle,
    Runtime,
};

use crate::{
    Error,
    Result,
};

tauri::ios_plugin_binding!(init_plugin_pipedash_push);

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>, api: PluginApi<R, C>,
) -> Result<PipedashPush<R>> {
    let handle = api
        .register_ios_plugin(init_plugin_pipedash_push)
        .map_err(|e| Error::Registration(e.to_string()))?;
    Ok(PipedashPush(handle))
}

pub struct PipedashPush<R: Runtime>(PluginHandle<R>);

#[derive(Deserialize)]
struct TokenResponse {
    token: String,
}

impl<R: Runtime> PipedashPush<R> {
    pub fn register_for_remote_notifications(&self) -> Result<String> {
        self.0
            .run_mobile_plugin::<TokenResponse>("register", ())
            .map(|resp| resp.token)
            .map_err(|e| match e {
                PluginInvokeError::InvokeRejected(resp)
                    if resp.code.as_deref() == Some("permissionDenied") =>
                {
                    Error::PermissionDenied
                }
                PluginInvokeError::InvokeRejected(resp) => {
                    Error::Registration(resp.message.unwrap_or_default())
                }
                other => Error::Registration(other.to_string()),
            })
    }
}
