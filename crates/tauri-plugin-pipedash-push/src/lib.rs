use tauri::{
    plugin::{
        Builder,
        TauriPlugin,
    },
    Manager,
    Runtime,
};

#[cfg(not(target_os = "ios"))]
mod commands;
mod error;
#[cfg(target_os = "ios")]
mod mobile;
#[cfg(not(target_os = "ios"))]
mod unsupported;

pub use error::{
    Error,
    Result,
};
#[cfg(target_os = "ios")]
pub use mobile::PipedashPush;
#[cfg(not(target_os = "ios"))]
pub use unsupported::PipedashPush;

pub trait PipedashPushExt<R: Runtime> {
    fn pipedash_push(&self) -> &PipedashPush<R>;
}

impl<R: Runtime, T: Manager<R>> PipedashPushExt<R> for T {
    fn pipedash_push(&self) -> &PipedashPush<R> {
        self.state::<PipedashPush<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    let builder = Builder::new("pipedash-push");

    #[cfg(not(target_os = "ios"))]
    let builder = builder.invoke_handler(tauri::generate_handler![commands::register]);

    builder
        .setup(|app, api| {
            #[cfg(target_os = "ios")]
            let push = mobile::init(app, api)?;
            #[cfg(not(target_os = "ios"))]
            let push = unsupported::init(app, api)?;
            app.manage(push);
            Ok(())
        })
        .build()
}
