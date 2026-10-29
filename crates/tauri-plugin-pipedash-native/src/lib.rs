use tauri::{
    plugin::{
        Builder,
        TauriPlugin,
    },
    Manager,
    Runtime,
};

mod error;
#[cfg(mobile)]
mod mobile;
mod model;
#[cfg(not(mobile))]
mod unsupported;

pub use error::{
    Error,
    Result,
};
#[cfg(mobile)]
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
#[cfg(not(mobile))]
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
            #[cfg(mobile)]
            let native = mobile::init(app, api)?;
            #[cfg(not(mobile))]
            let native = unsupported::init(app, api)?;
            app.manage(native);
            Ok(())
        })
        .build()
}
