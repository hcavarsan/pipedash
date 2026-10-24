use tauri::{
    command,
    AppHandle,
    Runtime,
};

use crate::{
    PipedashPushExt,
    Result,
};

#[command]
pub(crate) async fn register<R: Runtime>(app: AppHandle<R>) -> Result<String> {
    app.pipedash_push().register_for_remote_notifications()
}
