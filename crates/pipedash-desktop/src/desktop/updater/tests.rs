use super::*;

fn found() -> UpdateStatus {
    UpdateStatus {
        current_version: "0.13.0".to_string(),
        ..Default::default()
    }
    .available("0.14.0".to_string(), Some("Faster refresh".to_string()))
}

#[test]
fn automatic_checks_need_the_setting_and_a_settled_phase() {
    assert!(!should_auto_check(false, UpdatePhase::Idle));
    for phase in [
        UpdatePhase::Idle,
        UpdatePhase::UpToDate,
        UpdatePhase::Failed,
    ] {
        assert!(should_auto_check(true, phase), "{phase:?}");
    }
    for phase in [
        UpdatePhase::Checking,
        UpdatePhase::Available,
        UpdatePhase::Downloading,
        UpdatePhase::Installing,
    ] {
        assert!(!should_auto_check(true, phase), "{phase:?}");
    }
}

#[test]
fn every_install_step_keeps_the_found_update() {
    let found = found();
    for (status, phase) in [
        (found.checking(), UpdatePhase::Checking),
        (found.downloading(), UpdatePhase::Downloading),
        (found.installing(), UpdatePhase::Installing),
        (found.install_failed(), UpdatePhase::Failed),
    ] {
        assert_eq!(status.phase, phase);
        assert_eq!(status.current_version, "0.13.0");
        assert_eq!(status.version, found.version, "{phase:?}");
        assert_eq!(status.notes, found.notes, "{phase:?}");
    }
}

#[test]
fn a_failed_check_keeps_the_found_update_and_a_clean_check_drops_it() {
    let failed = found().check_failed();
    assert_eq!(failed.phase, UpdatePhase::Failed);
    assert_eq!(failed.version.as_deref(), Some("0.14.0"));
    assert_eq!(failed.notes.as_deref(), Some("Faster refresh"));
    let latest = failed.up_to_date();
    assert_eq!(latest.phase, UpdatePhase::UpToDate);
    assert_eq!(latest.current_version, "0.13.0");
    assert_eq!(latest.version, None);
    assert_eq!(latest.notes, None);
}

#[test]
fn progress_accumulates_chunks_and_a_retry_starts_from_zero() {
    let mut status = found().downloading();
    status.progress(700, Some(2_000));
    status.progress(300, Some(2_000));
    assert_eq!(status.downloaded, 1_000);
    assert_eq!(status.total, Some(2_000));
    let retry = status.install_failed().downloading();
    assert_eq!(retry.downloaded, 0);
    assert_eq!(retry.total, None);
}

#[test]
fn phases_use_snake_case_on_the_wire() {
    assert_eq!(
        serde_json::to_value(found().up_to_date()).unwrap()["phase"],
        "up_to_date"
    );
}
