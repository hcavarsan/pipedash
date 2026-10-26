use tauri::{
    plugin::{
        Builder,
        TauriPlugin,
    },
    Manager,
    Runtime,
};

mod error;
#[cfg(target_os = "ios")]
mod mobile;
mod model;
#[cfg(not(target_os = "ios"))]
mod unsupported;

pub use error::{
    Error,
    Result,
};
#[cfg(target_os = "ios")]
pub use mobile::PipedashNative;
pub use model::{
    Accessibility,
    Appearance,
    BuildWatchMonitoring,
    BuildWatchRunStatus,
    BuildWatchSnapshot,
    BuildWatchStatus,
    BuildWatchStopReason,
    HapticKind,
    LocalNotificationSettings,
    NotificationRefresh,
    NotificationResponse,
    PrivacyCover,
    ShareRect,
};
#[cfg(not(target_os = "ios"))]
pub use unsupported::PipedashNative;

pub trait PipedashNativeExt<R: Runtime> {
    fn pipedash_native(&self) -> &PipedashNative<R>;
}

impl<R: Runtime, T: Manager<R>> PipedashNativeExt<R> for T {
    fn pipedash_native(&self) -> &PipedashNative<R> {
        self.state::<PipedashNative<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("pipedash-native")
        .setup(|app, api| {
            #[cfg(target_os = "ios")]
            let native = mobile::init(app, api)?;
            #[cfg(not(target_os = "ios"))]
            let native = unsupported::init(app, api)?;
            app.manage(native);
            Ok(())
        })
        .build()
}
