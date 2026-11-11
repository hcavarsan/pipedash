use std::sync::Arc;
use std::time::Duration;

use pipedash_core::domain::ServerUrl;
use serde_json::json;

use super::commands::{
    is_ticket,
    validate_request,
};
use super::connection::RemoteConnection;
use super::model::{
    ConnectionInfo,
    RemoteError,
    RemoteStatus,
};
use super::pairing::{
    check_transport,
    name_for,
};
use super::session::Session;
use super::startup_mode;
use super::store::{
    self,
    HostKind,
    SavedConnection,
    SavedMode,
};
use crate::error::ErrorResponse;

fn url(input: &str) -> ServerUrl {
    ServerUrl::parse(input).unwrap()
}

fn client(server_url: &str, host_name: Option<&str>) -> SavedConnection {
    SavedConnection {
        mode: SavedMode::Client {
            server_url: server_url.to_string(),
            device_id: "d1".to_string(),
            host_kind: HostKind::Desktop,
            host_name: host_name.map(str::to_string),
        },
        allow_private_http: true,
        alerts_seen: None,
        preferences_pending: false,
    }
}

#[test]
fn a_saved_connection_wins_over_the_files_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "").unwrap();
    let saved = client("https://ci.example.com", None);
    store::write(dir.path(), &saved).unwrap();

    assert_eq!(startup_mode(dir.path(), &config), saved);
}

#[tokio::test]
async fn unsent_notification_preferences_stay_pending_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let remote = RemoteConnection::new(SavedConnection::default());
    remote.lock().saved = client("https://ci.example.com", None);

    remote.set_preferences_pending(dir.path(), true).await;
    let restarted = startup_mode(dir.path(), &config);
    assert!(restarted.preferences_pending);

    remote.set_preferences_pending(dir.path(), false).await;
    assert!(!startup_mode(dir.path(), &config).preferences_pending);

    let standalone = RemoteConnection::new(SavedConnection::default());
    standalone.set_preferences_pending(dir.path(), true).await;
    assert!(!standalone.preferences_pending());
}

#[test]
fn an_existing_install_without_a_saved_connection_stays_standalone() {
    for file in ["config.toml", "pipedash.db", "storage_config.json"] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(file), "").unwrap();

        let saved = startup_mode(dir.path(), &dir.path().join("config.toml"));

        assert_eq!(saved.mode, SavedMode::Standalone, "{file}");
        assert!(!dir.path().join("connection.json").exists());
    }
}

#[test]
fn an_empty_data_directory_waits_for_onboarding_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();

    let saved = startup_mode(dir.path(), &dir.path().join("config.toml"));

    assert_eq!(saved, SavedConnection::default());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn plain_http_needs_a_private_address_and_the_setting() {
    assert!(check_transport(&url("https://ci.example.com"), false).is_ok());
    assert!(check_transport(&url("https://8.8.8.8"), false).is_ok());
    assert!(check_transport(&url("http://192.168.1.20:8080"), true).is_ok());
    assert!(check_transport(&url("http://mac.local:8080"), true).is_ok());
    assert!(matches!(
        check_transport(&url("http://192.168.1.20:8080"), false),
        Err(RemoteError::PrivateHttpDisabled)
    ));
    assert!(matches!(
        check_transport(&url("http://example.com"), true),
        Err(RemoteError::PublicHttp)
    ));
    assert!(matches!(
        check_transport(&url("http://8.8.8.8"), false),
        Err(RemoteError::PublicHttp)
    ));
}

#[test]
fn requests_need_a_known_method_and_an_absolute_path() {
    for (method, path) in [
        ("GET", "/providers"),
        ("POST", "/providers/1/refresh"),
        ("PUT", "/x"),
        ("PATCH", "/x"),
        ("DELETE", "/devices/d1"),
    ] {
        assert!(validate_request(method, path).is_ok(), "{method} {path}");
    }
    for (method, path) in [
        ("GET", "//evil.example.com/x"),
        ("GET", "//x"),
        ("GET", "x"),
        ("GET", ""),
        ("get", "/x"),
        ("CONNECT", "/x"),
        ("TRACE", "/x"),
    ] {
        assert!(
            matches!(
                validate_request(method, path),
                Err(RemoteError::InvalidRequest)
            ),
            "{method} {path}"
        );
    }
}

#[test]
fn download_tickets_are_32_lowercase_hex_characters() {
    assert!(is_ticket(&"a1".repeat(16)));
    for ticket in [
        "",
        "abc",
        &"A1".repeat(16),
        &"g1".repeat(16),
        &"a1".repeat(17),
    ] {
        assert!(!is_ticket(ticket), "{ticket}");
    }
}

#[test]
fn the_device_name_is_the_host_name_or_a_platform_label() {
    assert_eq!(
        name_for("  Henrique's MacBook  ", "macos"),
        "Henrique's MacBook"
    );
    assert_eq!(
        name_for("Henriques-MacBook-Pro.local", "macos"),
        "Henriques-MacBook-Pro"
    );
    assert_eq!(name_for("", "macos"), "Mac");
    assert_eq!(name_for("localhost", "ios"), "iPhone or iPad");
    assert_eq!(name_for("LocalHost", "android"), "Android device");
    assert_eq!(name_for(" ", "windows"), "Windows PC");
    assert_eq!(name_for("", "linux"), "Linux PC");
    assert_eq!(name_for(&"é".repeat(100), "linux").chars().count(), 64);
}

#[test]
fn the_frontend_reads_the_connection_as_camel_case() {
    let remote = RemoteConnection::new(SavedConnection::default());
    assert_eq!(
        serde_json::to_value(remote.info()).unwrap(),
        json!({ "mode": "unset", "allowPrivateHttp": false })
    );

    let info = ConnectionInfo::new(
        &client("http://192.168.1.20:8080", Some("Studio")),
        RemoteStatus::Reconnecting,
    );
    assert_eq!(
        serde_json::to_value(info).unwrap(),
        json!({
            "mode": "client",
            "serverUrl": "http://192.168.1.20:8080",
            "host": "Studio",
            "hostKind": "desktop",
            "deviceId": "d1",
            "status": "reconnecting",
            "allowPrivateHttp": true,
        })
    );
    assert_eq!(
        ConnectionInfo::new(
            &client("https://ci.example.com:8443", None),
            RemoteStatus::Removed
        )
        .host
        .as_deref(),
        Some("ci.example.com")
    );
}

fn waiting_remote() -> RemoteConnection {
    let remote = RemoteConnection::new(SavedConnection::default());
    {
        let mut inner = remote.lock();
        inner.saved = client("https://ci.example.com", Some("Studio"));
        inner.status = RemoteStatus::Connecting;
    }
    remote
}

#[test]
fn only_the_current_session_can_change_the_status() {
    let remote = RemoteConnection::new(SavedConnection::default());
    let (outgoing, _queue) = tokio::sync::mpsc::channel(1);
    let session = Arc::new(Session::new("host", outgoing));
    remote.lock().generation = 2;

    assert!(!remote.attach(1, Arc::clone(&session)));
    assert_eq!(remote.lost(1), None);
    assert!(!remote.mark_removed(1));
    assert_eq!(remote.lock().status, RemoteStatus::Connecting);

    assert!(remote.attach(2, session));
    assert_eq!(remote.lock().status, RemoteStatus::Connected);
    assert_eq!(remote.lost(2), Some(1));
    assert_eq!(remote.lost(2), Some(2));
    assert_eq!(remote.lock().status, RemoteStatus::Reconnecting);
    assert!(remote.lock().session.is_none());
    assert!(remote.mark_removed(2));
    assert_eq!(remote.lock().status, RemoteStatus::Removed);
}

#[tokio::test]
async fn a_request_issued_while_connecting_waits_for_the_session() {
    let remote = Arc::new(waiting_remote());
    let (outgoing, _queue) = tokio::sync::mpsc::channel(1);

    let asking = Arc::clone(&remote);
    let ready = tokio::spawn(async move { asking.ready_session(Duration::from_secs(5)).await });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!ready.is_finished());

    assert!(remote.attach(0, Arc::new(Session::new("host", outgoing))));

    assert!(ready.await.unwrap().is_ok());
}

#[tokio::test]
async fn a_request_gives_up_as_unreachable_when_nothing_connects() {
    let remote = waiting_remote();
    for status in [RemoteStatus::Connecting, RemoteStatus::Reconnecting] {
        remote.lock().status = status;

        let Err(error) = remote.ready_session(Duration::from_millis(50)).await else {
            panic!("nothing connected");
        };

        assert!(matches!(error, RemoteError::Unreachable(host) if host == "Studio"));
    }
}

#[tokio::test]
async fn a_removed_device_and_a_standalone_app_fail_without_waiting() {
    let remote = waiting_remote();
    assert!(remote.mark_removed(0));
    let Err(removed) = remote.ready_session(Duration::from_secs(30)).await else {
        panic!("a removed device has no session");
    };
    assert!(matches!(removed, RemoteError::Unreachable(_)));

    let standalone = RemoteConnection::new(SavedConnection::default());
    let Err(error) = standalone.ready_session(Duration::from_secs(30)).await else {
        panic!("standalone has no server");
    };
    assert!(matches!(error, RemoteError::NotConnected));
}

#[test]
fn errors_carry_a_code_the_frontend_can_branch_on() {
    let response = ErrorResponse::from(RemoteError::Unreachable("mac.local".to_string()));
    assert_eq!(response.code.as_deref(), Some("UNREACHABLE"));
    assert_eq!(
        response.error,
        "Couldn't reach mac.local. Check the address and that this device can reach it."
    );

    let response = ErrorResponse::from(RemoteError::Rejected {
        code: "PAIRING_CODE_INVALID".to_string(),
        message: "This code expired or was already used. Create a new code on the server."
            .to_string(),
    });
    assert_eq!(response.code.as_deref(), Some("PAIRING_CODE_INVALID"));
    assert!(response.error.starts_with("This code expired"));
}
