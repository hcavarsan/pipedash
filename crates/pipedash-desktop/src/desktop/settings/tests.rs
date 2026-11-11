use super::*;

#[test]
fn disabling_tray_disables_background_but_preserves_notification_consent() {
    let saved = SavedSettings {
        settings: DesktopSettings {
            show_tray: true,
            keep_running_on_close: true,
            notifications_enabled: true,
            auto_check_updates: true,
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

#[test]
fn automatic_update_checks_default_on_and_keep_an_explicit_off() {
    let missing: SavedSettings = serde_json::from_str("{}").unwrap();
    assert!(missing.settings.auto_check_updates);
    let off: SavedSettings = serde_json::from_str(r#"{"auto_check_updates":false}"#).unwrap();
    assert!(!off.settings.auto_check_updates);
    let request = DesktopSettingsPatch {
        auto_check_updates: Some(false),
        ..Default::default()
    };
    let mut retry = SettingsRetry::default();
    retry.failed(request, "Save failed");
    let retried = retry.request(DesktopSettingsPatch::default());
    assert_eq!(retried.auto_check_updates, Some(false));
    assert!(!missing.patch(retried).unwrap().settings.auto_check_updates);
    retry.committed(retried);
    assert!(retry.request(DesktopSettingsPatch::default()).is_empty());
    assert!(retry.error.is_none());
}
