use std::path::Path;

use serde::{
    Deserialize,
    Serialize,
};
use tauri::{
    AppHandle,
    Manager,
};

use super::{
    menu,
    on_main,
    projection::{
        refresh_projection,
        wake,
    },
    ready,
    show_main_window,
    state::{
        current_status,
        failure,
        publish_status,
        DesktopState,
        DesktopStatus,
    },
};
use crate::{
    error::ErrorResponse,
    native::NotificationTarget,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DesktopSettings {
    pub show_tray: bool,
    pub keep_running_on_close: bool,
    pub notifications_enabled: bool,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DesktopSettingsPatch {
    pub show_tray: Option<bool>,
    pub keep_running_on_close: Option<bool>,
    pub notifications_enabled: Option<bool>,
}

impl DesktopSettingsPatch {
    pub fn is_empty(self) -> bool {
        self.show_tray.is_none()
            && self.keep_running_on_close.is_none()
            && self.notifications_enabled.is_none()
    }
}

#[derive(Default)]
pub(super) struct SettingsRetry {
    patch: DesktopSettingsPatch,
    pub error: Option<String>,
}

impl SettingsRetry {
    pub fn request(&self, patch: DesktopSettingsPatch) -> DesktopSettingsPatch {
        if patch.is_empty() {
            self.patch
        } else {
            patch
        }
    }

    pub fn failed(&mut self, patch: DesktopSettingsPatch, error: &str) {
        self.patch.show_tray = patch.show_tray.or(self.patch.show_tray);
        self.patch.keep_running_on_close = if patch.show_tray == Some(false) {
            Some(false)
        } else {
            patch
                .keep_running_on_close
                .or(self.patch.keep_running_on_close)
        };
        self.patch.notifications_enabled = patch
            .notifications_enabled
            .or(self.patch.notifications_enabled);
        self.error = (!self.patch.is_empty()).then(|| error.to_string());
    }

    pub fn committed(&mut self, patch: DesktopSettingsPatch) {
        if patch.show_tray.is_some() {
            self.patch.show_tray = None;
        }
        if patch.keep_running_on_close.is_some() || patch.show_tray == Some(false) {
            self.patch.keep_running_on_close = None;
        }
        if patch.notifications_enabled.is_some() {
            self.patch.notifications_enabled = None;
        }
        if self.patch.is_empty() {
            self.error = None;
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct SavedSettings {
    #[serde(flatten)]
    pub settings: DesktopSettings,
    pub notification_preference_imported: bool,
}

impl SavedSettings {
    pub fn patch(self, patch: DesktopSettingsPatch) -> Result<Self, ErrorResponse> {
        let mut next = self;
        if let Some(enabled) = patch.show_tray {
            next.settings.show_tray = enabled;
        }
        if let Some(enabled) = patch.keep_running_on_close {
            next.settings.keep_running_on_close = enabled;
        }
        if !next.settings.show_tray {
            if patch.keep_running_on_close == Some(true) {
                return Err(failure("Turn on the tray before enabling background mode"));
            }
            next.settings.keep_running_on_close = false;
        }
        if let Some(enabled) = patch.notifications_enabled {
            next.settings.notifications_enabled = enabled;
            next.notification_preference_imported = true;
        }
        Ok(next)
    }
}

pub(super) fn load(path: &Path) -> (SavedSettings, Option<String>) {
    match std::fs::read(path) {
        Ok(bytes) => match serde_json::from_slice::<SavedSettings>(&bytes) {
            Ok(mut saved) => {
                if !saved.settings.show_tray {
                    saved.settings.keep_running_on_close = false;
                }
                (saved, None)
            }
            Err(_) => (
                SavedSettings::default(),
                Some("Desktop settings are invalid. Restore desktop.json and retry, or change a setting to save new preferences.".to_string()),
            ),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            (SavedSettings::default(), None)
        }
        Err(_) => (
            SavedSettings::default(),
            Some("Couldn't read desktop settings. Check file permissions and retry.".to_string()),
        ),
    }
}

pub(super) fn save(path: &Path, saved: SavedSettings) -> std::io::Result<()> {
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, serde_json::to_vec(&saved)?)?;
    std::fs::rename(temporary, path)
}

async fn persist(app: &AppHandle, saved: SavedSettings) -> Result<(), ErrorResponse> {
    let file = app.state::<DesktopState>().path.clone();
    tauri::async_runtime::spawn_blocking(move || save(&file, saved))
        .await
        .map_err(|_| failure("Desktop settings save was interrupted"))?
        .map_err(|_| failure("Couldn't save desktop settings. Check file permissions and retry."))
}

async fn try_commit_settings(app: &AppHandle, next: SavedSettings) -> Result<(), ErrorResponse> {
    let (previous, was_ready, settings_error) = {
        let state = app.state::<DesktopState>();
        let inner = state.lock();
        (
            inner.saved,
            inner.tray_ready,
            inner.settings_error.is_some(),
        )
    };
    let created = next.settings.show_tray && !was_ready;
    if created {
        if let Err(error) = on_main(app, menu::create_tray).await {
            app.state::<DesktopState>().lock().tray_error = Some(error.error.clone());
            return Err(error);
        }
    }
    if !next.settings.show_tray && was_ready {
        on_main(app, show_main_window).await?;
    }
    if next != previous || settings_error {
        if let Err(error) = persist(app, next).await {
            if created {
                let _ = on_main(app, menu::remove_tray).await;
            }
            app.state::<DesktopState>().lock().settings_error = Some(error.error.clone());
            return Err(error);
        }
    }
    {
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        inner.saved = next;
        inner.settings_error = None;
        inner.tray_error = None;
        inner.tray_ready = next.settings.show_tray && (was_ready || created);
    }
    if !next.settings.show_tray && was_ready {
        if let Err(error) = on_main(app, menu::remove_tray).await {
            app.state::<DesktopState>().lock().tray_error = Some(error.error);
        }
    }
    wake(app);
    Ok(())
}

pub(super) async fn commit_settings(
    app: &AppHandle, next: SavedSettings, request: DesktopSettingsPatch,
) -> Result<DesktopStatus, ErrorResponse> {
    let result = try_commit_settings(app, next).await;
    {
        let state = app.state::<DesktopState>();
        let mut inner = state.lock();
        match &result {
            Ok(_) => inner.retry.committed(request),
            Err(error) => inner.retry.failed(request, &error.error),
        }
    }
    publish_status(app);
    result.map(|_| current_status(app))
}

#[tauri::command]
pub fn get_desktop_status(app: AppHandle) -> DesktopStatus {
    current_status(&app)
}

#[tauri::command]
pub async fn update_desktop_settings(
    app: AppHandle, patch: DesktopSettingsPatch,
) -> Result<DesktopStatus, ErrorResponse> {
    let state = app.state::<DesktopState>();
    let _guard = state.settings_gate.lock().await;
    let request = state.lock().retry.request(patch);
    if request.is_empty() && state.lock().settings_error.is_some() {
        let path = state.path.clone();
        let (saved, error) = tauri::async_runtime::spawn_blocking(move || load(&path))
            .await
            .map_err(|_| failure("Couldn't reread desktop settings"))?;
        let failed = error.is_some();
        {
            let mut inner = state.lock();
            inner.settings_error = error;
            if !failed {
                inner.saved = saved;
            }
        }
        if failed {
            publish_status(&app);
            refresh_projection(&app).await;
            return Ok(current_status(&app));
        }
    }
    let next = state.lock().saved.patch(request)?;
    commit_settings(&app, next, request).await?;
    refresh_projection(&app).await;
    Ok(current_status(&app))
}

#[tauri::command]
pub async fn import_desktop_notification_preference(
    app: AppHandle, enabled: bool,
) -> Result<DesktopStatus, ErrorResponse> {
    let state = app.state::<DesktopState>();
    let _guard = state.settings_gate.lock().await;
    if state.lock().settings_error.is_some() {
        return Ok(current_status(&app));
    }
    let mut next = state.lock().saved;
    if next.notification_preference_imported {
        return Ok(current_status(&app));
    }
    next.notification_preference_imported = true;
    next.settings.notifications_enabled = enabled;
    commit_settings(
        &app,
        next,
        DesktopSettingsPatch {
            notifications_enabled: Some(enabled),
            ..Default::default()
        },
    )
    .await
}

pub(crate) async fn reset_settings(app: &AppHandle) -> Result<(), ErrorResponse> {
    let state = app.state::<DesktopState>();
    let _guard = state.settings_gate.lock().await;
    commit_settings(
        app,
        SavedSettings {
            notification_preference_imported: true,
            ..Default::default()
        },
        DesktopSettingsPatch {
            show_tray: Some(false),
            keep_running_on_close: Some(false),
            notifications_enabled: Some(false),
        },
    )
    .await
    .map(|_| ())
}

#[tauri::command]
pub fn test_desktop_notification(app: AppHandle) -> Result<(), ErrorResponse> {
    if !ready(&app) {
        return Err(failure(
            "Unlock and initialize Pipedash before sending a test",
        ));
    }
    if !app
        .state::<DesktopState>()
        .lock()
        .saved
        .settings
        .notifications_enabled
    {
        return Err(failure(
            "Turn on system notifications before sending a test",
        ));
    }
    crate::system_notifications::send(
        &app,
        "Pipedash notification test",
        "System notifications are configured for this Pipedash session.",
        Some(&NotificationTarget {
            key: Some("desktop-test".to_string()),
            ..Default::default()
        }),
    )
}

#[cfg(test)]
mod tests;
