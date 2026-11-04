use std::path::Path;

use serde::{
    Deserialize,
    Serialize,
};

use super::failure;
use crate::commands::ErrorResponse;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabling_tray_disables_background_but_preserves_notification_consent() {
        let saved = SavedSettings {
            settings: DesktopSettings {
                show_tray: true,
                keep_running_on_close: true,
                notifications_enabled: true,
            },
            notification_preference_imported: true,
        };
        let next = saved
            .patch(DesktopSettingsPatch {
                show_tray: Some(false),
                ..Default::default()
            })
            .unwrap();
        assert!(!next.settings.keep_running_on_close);
        assert!(next.settings.notifications_enabled);
        assert!(SavedSettings::default()
            .patch(DesktopSettingsPatch {
                keep_running_on_close: Some(true),
                ..Default::default()
            })
            .is_err());
    }

    #[test]
    fn malformed_settings_remain_untouched_until_an_explicit_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("desktop.json");
        std::fs::write(&path, b"broken").unwrap();
        let (saved, error) = load(&path);
        assert!(error.is_some());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken");
        save(&path, saved).unwrap();
        assert!(load(&path).1.is_none());
    }

    #[test]
    fn failed_save_does_not_replace_the_settings_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("desktop.json");
        save(&path, SavedSettings::default()).unwrap();
        std::fs::create_dir(path.with_extension("json.tmp")).unwrap();
        let enabled = SavedSettings::default()
            .patch(DesktopSettingsPatch {
                notifications_enabled: Some(true),
                ..Default::default()
            })
            .unwrap();
        assert!(save(&path, enabled).is_err());
        assert!(!load(&path).0.notification_preference_imported);
    }

    #[test]
    fn retry_reapplies_failed_enable_without_reverting_later_notification_consent() {
        let enable = DesktopSettingsPatch {
            show_tray: Some(true),
            ..Default::default()
        };
        let notifications = DesktopSettingsPatch {
            notifications_enabled: Some(true),
            ..Default::default()
        };
        let mut retry = SettingsRetry::default();
        retry.failed(enable, "Tray unavailable");
        let saved = SavedSettings::default().patch(notifications).unwrap();
        retry.committed(notifications);
        assert!(retry.error.is_some());
        let request = retry.request(DesktopSettingsPatch::default());
        let next = saved.patch(request).unwrap();
        assert!(next.settings.show_tray);
        assert!(next.settings.notifications_enabled);
        retry.committed(request);
        assert!(retry.request(DesktopSettingsPatch::default()).is_empty());
        assert!(retry.error.is_none());
    }

    #[test]
    fn disabling_tray_cancels_pending_background_and_enable_requests() {
        let mut retry = SettingsRetry::default();
        retry.failed(
            DesktopSettingsPatch {
                show_tray: Some(true),
                keep_running_on_close: Some(true),
                ..Default::default()
            },
            "Save failed",
        );
        let disable = DesktopSettingsPatch {
            show_tray: Some(false),
            ..Default::default()
        };
        retry.failed(disable, "Still read-only");
        let request = retry.request(DesktopSettingsPatch::default());
        let next = SavedSettings::default().patch(request).unwrap();
        assert!(!next.settings.show_tray);
        assert!(!next.settings.keep_running_on_close);
        retry.committed(disable);
        assert!(retry.request(DesktopSettingsPatch::default()).is_empty());
        assert!(retry.error.is_none());
    }
}
