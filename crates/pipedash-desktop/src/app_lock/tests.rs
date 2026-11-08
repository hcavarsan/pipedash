use pipedash_core::application::services::storage_service::vault::UnlockVaultResponse;

use super::commands::{
    disable_app_lock,
    enable_confirmed,
};
use super::*;
use crate::error::ErrorResponse;

const ON: AppLockSettings = AppLockSettings {
    enabled: true,
    lock_on_background: true,
};

#[test]
fn the_app_starts_locked_only_when_supported_enabled_and_set_up() {
    assert!(starts_locked(true, ON, true));
    assert!(!starts_locked(false, ON, true));
    assert!(!starts_locked(true, AppLockSettings::default(), true));
    assert!(!starts_locked(true, ON, false));
}

#[test]
fn resuming_locks_only_a_loaded_idle_core_with_background_lock_on() {
    assert!(resume_should_lock(ON, false, 0, true));
    assert!(!resume_should_lock(ON, true, 0, true));
    assert!(!resume_should_lock(ON, false, 1, true));
    assert!(!resume_should_lock(ON, false, 0, false));
    let foreground_only = AppLockSettings {
        enabled: true,
        lock_on_background: false,
    };
    assert!(!resume_should_lock(foreground_only, false, 0, true));
    assert!(!resume_should_lock(
        AppLockSettings::default(),
        false,
        0,
        true
    ));
}

#[test]
fn a_biometric_prompt_blocks_locking_until_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppLockState::load(dir.path(), true, true);
    state.save_settings(ON).unwrap();
    state.set_locked(false);

    let prompt = state.begin_prompt();
    let second = state.begin_prompt();
    assert!(!state.should_lock_on_resume(true));
    drop(prompt);
    assert!(!state.should_lock_on_resume(true));
    drop(second);
    assert!(state.should_lock_on_resume(true));
}

#[test]
fn settings_survive_a_reload_and_bad_files_fall_back_to_off() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppLockState::load(dir.path(), true, true);
    assert!(!state.locked());
    state.save_settings(ON).unwrap();

    let reloaded = AppLockState::load(dir.path(), true, true);
    assert_eq!(reloaded.settings(), ON);
    assert!(reloaded.locked());

    std::fs::write(dir.path().join(SETTINGS_FILE), b"{ not json").unwrap();
    assert_eq!(
        read_settings(&dir.path().join(SETTINGS_FILE)),
        AppLockSettings::default()
    );
}

#[test]
fn failed_settings_deletion_preserves_state_for_retry() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppLockState::load(dir.path(), true, true);
    state.save_settings(ON).unwrap();
    std::fs::remove_file(&state.path).unwrap();
    std::fs::create_dir(&state.path).unwrap();

    assert!(state.clear_settings().is_err());
    assert_eq!(state.settings(), ON);

    std::fs::remove_dir(&state.path).unwrap();
    state.clear_settings().unwrap();
    state.clear_settings().unwrap();
    assert_eq!(state.settings(), AppLockSettings::default());
}

#[test]
fn a_failed_password_deletion_keeps_app_lock_enabled_for_retry() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppLockState::load(dir.path(), true, true);
    state.save_settings(ON).unwrap();
    let off = AppLockSettings::default();

    assert!(disable_app_lock(&state, ON, off, || Err("keychain locked")).is_err());
    assert_eq!(AppLockState::load(dir.path(), true, true).settings(), ON);

    let mut deleted = false;
    disable_app_lock(&state, ON, off, || {
        deleted = true;
        Ok::<(), &str>(())
    })
    .unwrap();
    assert!(deleted);
    assert_eq!(AppLockState::load(dir.path(), true, true).settings(), off);
}

#[test]
fn a_failed_settings_write_keeps_the_saved_password() {
    let dir = tempfile::tempdir().unwrap();
    let state = AppLockState::load(dir.path(), true, true);
    state.save_settings(ON).unwrap();
    std::fs::remove_file(&state.path).unwrap();
    std::fs::create_dir(&state.path).unwrap();

    let mut deleted = false;
    assert!(
        disable_app_lock(&state, ON, AppLockSettings::default(), || {
            deleted = true;
            Ok::<(), &str>(())
        })
        .is_err()
    );
    assert!(!deleted);
    assert_eq!(state.settings(), ON);
}

#[test]
fn unsupported_platforms_report_no_app_lock_but_keep_the_session_flag() {
    let info = BiometryInfo {
        biometry: Biometry::FaceId,
        available: true,
    };
    let status = AppLockStatus::new(false, info, ON, true);
    assert_eq!(
        status,
        AppLockStatus {
            supported: false,
            biometry: Biometry::None,
            biometry_available: false,
            enabled: false,
            lock_on_background: false,
            locked: true,
        }
    );
}

#[test]
fn supported_platforms_report_settings_and_biometry() {
    let info = BiometryInfo {
        biometry: Biometry::FaceId,
        available: true,
    };
    let status = AppLockStatus::new(true, info, ON, false);
    assert_eq!(
        serde_json::to_value(status).unwrap(),
        serde_json::json!({
            "supported": true,
            "biometry": "faceId",
            "biometry_available": true,
            "enabled": true,
            "lock_on_background": true,
            "locked": false,
        })
    );
    assert_eq!(
        serde_json::to_value(Biometry::TouchId).unwrap(),
        serde_json::json!("touchId")
    );
}

#[test]
fn only_a_cancelled_unlock_carries_a_reason() {
    assert_eq!(
        serde_json::to_value(UnlockVaultResponse::cancelled()).unwrap(),
        serde_json::json!({ "success": false, "message": "cancelled", "reason": "cancelled" })
    );
    assert_eq!(
        serde_json::to_value(UnlockVaultResponse::failed("nope")).unwrap(),
        serde_json::json!({ "success": false, "message": "nope" })
    );
}

#[test]
fn cancel_codes_are_told_apart_from_real_failures() {
    for code in CANCEL_CODES {
        assert_eq!(classify_rejection(Some(code), "x"), AuthOutcome::Cancelled);
    }
    assert_eq!(
        classify_rejection(Some("biometryLockout"), "Locked out"),
        AuthOutcome::Failed("Locked out".to_string())
    );
    assert_eq!(
        classify_rejection(None, "boom"),
        AuthOutcome::Failed("boom".to_string())
    );
}

#[test]
fn cancelling_the_enable_prompt_is_not_an_error() {
    assert!(matches!(
        enable_confirmed(AuthOutcome::Authenticated),
        Ok(true)
    ));
    assert!(matches!(
        enable_confirmed(AuthOutcome::Cancelled),
        Ok(false)
    ));
    let failed = enable_confirmed(AuthOutcome::Failed("Locked out".to_string()));
    assert!(matches!(failed, Err(ErrorResponse { error, .. }) if error == "Locked out"));
}
