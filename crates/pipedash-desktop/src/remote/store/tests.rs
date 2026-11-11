use super::*;

fn client() -> SavedConnection {
    SavedConnection {
        mode: SavedMode::Client {
            server_url: "http://192.168.1.20:8080".to_string(),
            device_id: "d1".to_string(),
            host_kind: HostKind::Desktop,
            host_name: Some("Studio".to_string()),
        },
        allow_private_http: true,
        alerts_seen: Some(7),
        preferences_pending: true,
    }
}

#[test]
fn every_mode_round_trips_through_the_file() {
    let dir = tempfile::tempdir().unwrap();
    for saved in [
        SavedConnection::default(),
        SavedConnection {
            mode: SavedMode::Standalone,
            allow_private_http: false,
            alerts_seen: None,
            preferences_pending: false,
        },
        client(),
    ] {
        write(dir.path(), &saved).unwrap();
        assert_eq!(read(dir.path()), Some(saved));
    }
    assert!(!dir.path().join(TMP_FILE_NAME).exists());
}

#[test]
fn the_file_is_flat_and_holds_no_token() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), &client()).unwrap();

    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join(FILE_NAME)).unwrap()).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "mode": "client",
            "server_url": "http://192.168.1.20:8080",
            "device_id": "d1",
            "host_kind": "desktop",
            "host_name": "Studio",
            "allow_private_http": true,
            "alerts_seen": 7,
            "preferences_pending": true,
        })
    );
}

#[test]
fn a_missing_or_unreadable_file_reads_as_none() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(read(dir.path()), None);

    fs::write(dir.path().join(FILE_NAME), b"{not json").unwrap();
    assert_eq!(read(dir.path()), None);
}
