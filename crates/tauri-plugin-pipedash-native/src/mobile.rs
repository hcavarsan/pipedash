use std::path::Path;

use serde::{
    de::DeserializeOwned,
    Deserialize,
    Serialize,
};
use tauri::{
    ipc::{
        Channel as IpcChannel,
        InvokeResponseBody,
    },
    plugin::{
        PluginApi,
        PluginHandle,
    },
    AppHandle,
    Runtime,
};

use crate::{
    Accessibility,
    Appearance,
    BuildWatchMonitoring,
    BuildWatchSnapshot,
    BuildWatchStatus,
    BuildWatchStopReason,
    Error,
    HapticKind,
    NotificationResponse,
    PrivacyCover,
    Result,
    ShareRect,
};

tauri::ios_plugin_binding!(init_plugin_pipedash_native);

pub fn init<R: Runtime, C: DeserializeOwned>(
    _app: &AppHandle<R>, api: PluginApi<R, C>,
) -> Result<PipedashNative<R>> {
    let handle = api
        .register_ios_plugin(init_plugin_pipedash_native)
        .map_err(|e| Error::Native(e.to_string()))?;
    Ok(PipedashNative(handle))
}

pub struct PipedashNative<R: Runtime>(PluginHandle<R>);

#[derive(Serialize)]
struct HapticArgs {
    kind: HapticKind,
}

#[derive(Serialize)]
struct AppearanceArgs {
    scheme: Appearance,
}

#[derive(Serialize)]
struct BadgeArgs {
    count: u32,
}

#[derive(Serialize)]
struct ShareArgs<'a> {
    path: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShareLinkArgs<'a> {
    url: &'a str,
    source_rect: ShareRect,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StopBuildWatchArgs<'a> {
    session_id: &'a str,
    reason: BuildWatchStopReason,
}

#[derive(Deserialize)]
struct ShareResult {
    completed: bool,
}

#[derive(Serialize)]
struct RegisterListener {
    event: &'static str,
    handler: IpcChannel,
}

impl<R: Runtime> PipedashNative<R> {
    async fn call<T: DeserializeOwned>(&self, command: &str, payload: impl Serialize) -> Result<T> {
        self.0
            .run_mobile_plugin_async(command, payload)
            .await
            .map_err(|e| Error::Native(e.to_string()))
    }

    fn listen<F: Fn(String) + Send + Sync + 'static>(
        &self, event: &'static str, handler: F,
    ) -> Result<()> {
        let channel = IpcChannel::new(move |body| {
            if let InvokeResponseBody::Json(payload) = body {
                handler(payload);
            }
            Ok(())
        });
        self.0
            .run_mobile_plugin::<()>(
                "registerListener",
                RegisterListener {
                    event,
                    handler: channel,
                },
            )
            .map_err(|e| Error::Native(e.to_string()))
    }

    pub async fn haptic(&self, kind: HapticKind) -> Result<()> {
        self.call("haptic", HapticArgs { kind }).await
    }

    pub async fn set_appearance(&self, scheme: Appearance) -> Result<()> {
        self.call("setAppearance", AppearanceArgs { scheme }).await
    }

    pub async fn accessibility(&self) -> Result<Accessibility> {
        self.call("accessibilityState", ()).await
    }

    pub async fn build_watch_status(&self) -> Result<BuildWatchStatus> {
        self.call("buildWatchStatus", ()).await
    }

    pub async fn start_build_watch(&self, watch: &BuildWatchSnapshot) -> Result<BuildWatchStatus> {
        watch.validate()?;
        if watch.monitoring != BuildWatchMonitoring::Watching {
            return Err(Error::Native(
                "Only an active build can be watched".to_string(),
            ));
        }
        self.call("startBuildWatch", watch).await
    }

    pub async fn update_build_watch(&self, watch: &BuildWatchSnapshot) -> Result<BuildWatchStatus> {
        watch.validate()?;
        self.call("updateBuildWatch", watch).await
    }

    pub async fn stop_build_watch(
        &self, session_id: &str, reason: BuildWatchStopReason,
    ) -> Result<BuildWatchStatus> {
        if session_id.trim().is_empty() || session_id.len() > 128 {
            return Err(Error::Native("Invalid build watch session".to_string()));
        }
        self.call("stopBuildWatch", StopBuildWatchArgs { session_id, reason })
            .await
    }

    pub async fn open_settings(&self) -> Result<()> {
        self.call("openSettings", ()).await
    }

    pub async fn set_badge(&self, count: u32) -> Result<()> {
        self.call("setBadge", BadgeArgs { count }).await
    }

    pub async fn configure_privacy_cover(&self, cover: PrivacyCover) -> Result<()> {
        self.call("configurePrivacyCover", cover).await
    }

    pub async fn release_privacy_cover(&self) -> Result<()> {
        self.call("releasePrivacyCover", ()).await
    }

    pub async fn share_file(&self, path: &Path) -> Result<bool> {
        let path = path
            .to_str()
            .ok_or_else(|| Error::Native("The file path isn't valid UTF-8".to_string()))?;
        let result: ShareResult = self.call("shareFile", ShareArgs { path }).await?;
        Ok(result.completed)
    }

    pub async fn share_link(&self, url: &str, source_rect: ShareRect) -> Result<bool> {
        let result: ShareResult = self
            .call("shareLink", ShareLinkArgs { url, source_rect })
            .await?;
        Ok(result.completed)
    }

    pub fn on_notification_response<F: Fn(NotificationResponse) + Send + Sync + 'static>(
        &self, handler: F,
    ) -> Result<()> {
        self.listen("notificationAction", move |payload| {
            if let Ok(response) = serde_json::from_str::<NotificationResponse>(&payload) {
                handler(response);
            }
        })
    }

    pub fn on_build_watch_changed<F: Fn(BuildWatchStatus) + Send + Sync + 'static>(
        &self, handler: F,
    ) -> Result<()> {
        self.listen("build-watch-changed", move |payload| {
            if let Ok(status) = serde_json::from_str::<BuildWatchStatus>(&payload) {
                handler(status);
            }
        })
    }

    pub fn on_status_bar_tap<F: Fn() + Send + Sync + 'static>(&self, handler: F) -> Result<()> {
        self.listen("statusBarTap", move |_| handler())
    }
}
