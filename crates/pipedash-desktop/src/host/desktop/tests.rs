use std::io::ErrorKind;

use super::settings::{
    SETTINGS_FILE,
    TMP_SETTINGS_FILE,
};
use super::*;

#[test]
fn settings_default_when_missing_or_unreadable_and_survive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(read_settings(dir.path()), HostSettings::default());

    std::fs::write(dir.path().join(SETTINGS_FILE), b"{oops").unwrap();
    assert_eq!(read_settings(dir.path()), HostSettings::default());

    let settings = HostSettings {
        enabled: true,
        port: Some(52411),
    };
    write_settings(dir.path(), settings).unwrap();
    assert_eq!(read_settings(dir.path()), settings);
    assert!(!dir.path().join(TMP_SETTINGS_FILE).exists());
}

#[test]
fn status_reports_what_the_settings_and_errors_say() {
    let dir = tempfile::tempdir().unwrap();
    let host = HostState::load(dir.path());
    assert_eq!(
        host.status(),
        SharingStatus {
            enabled: false,
            running: false,
            url: None,
            error: None,
            error_code: None,
        }
    );

    host.update_settings(|settings| settings.enabled = true)
        .unwrap();
    host.lock().error = Some(failure(
        "Port 1 is in use by another app.",
        Some(SharingErrorCode::PortInUse),
    ));
    let status = host.status();
    assert!(status.enabled && !status.running);
    assert_eq!(
        status.error.as_deref(),
        Some("Port 1 is in use by another app.")
    );
    assert_eq!(status.error_code, Some(SharingErrorCode::PortInUse));
    assert!(read_settings(dir.path()).enabled);
}

#[test]
fn failures_carry_a_code_the_frontend_can_branch_on() {
    let in_use = std::io::Error::from(ErrorKind::AddrInUse);
    let failed = bind_failure(Some(52411), &in_use);
    assert_eq!(failed.message, "Port 52411 is in use by another app.");
    assert_eq!(failed.code, Some(SharingErrorCode::PortInUse));

    assert_eq!(bind_failure(None, &in_use).code, None);
    let denied = std::io::Error::from(ErrorKind::PermissionDenied);
    assert_eq!(bind_failure(Some(80), &denied).code, None);

    let value = serde_json::to_value(SharingStatus {
        enabled: true,
        running: false,
        url: None,
        error: Some(NO_NETWORK.to_owned()),
        error_code: Some(SharingErrorCode::NoNetwork),
    })
    .unwrap();
    assert_eq!(value["errorCode"], "NO_NETWORK");
    assert_eq!(value["error"], NO_NETWORK);
}
