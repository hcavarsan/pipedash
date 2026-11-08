pub(crate) mod alerts;
pub(crate) mod commands;
pub(crate) mod connection;
pub(crate) mod model;
pub(crate) mod pairing;
mod session;
pub(crate) mod store;
#[cfg(test)]
pub(crate) mod test_host;

use std::path::Path;

use tauri::{
    AppHandle,
    Manager,
};

use self::connection::RemoteConnection;
use self::store::{
    SavedConnection,
    SavedMode,
};

pub fn startup_mode(dir: &Path, config_path: &Path) -> SavedConnection {
    store::read(dir).unwrap_or_else(|| SavedConnection {
        mode: if config_path.exists()
            || dir.join("pipedash.db").exists()
            || dir.join("storage_config.json").exists()
        {
            SavedMode::Standalone
        } else {
            SavedMode::Unset
        },
        allow_private_http: false,
        alerts_seen: None,
        preferences_pending: false,
    })
}

pub fn start(app: &AppHandle) {
    reconnect(app);
}

fn reconnect(app: &AppHandle) {
    let remote = app.state::<RemoteConnection>();
    if remote.can_reconnect() {
        remote.launch(app, None);
    }
}

#[cfg(mobile)]
pub fn handle_window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if matches!(event, tauri::WindowEvent::Resumed) {
        reconnect(window.app_handle());
    }
}

#[cfg(test)]
mod tests;
