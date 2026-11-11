use super::*;

#[test]
fn secret_users_are_per_server() {
    assert_eq!(Secret::ApiToken.user("ab"), "push_server_token_ab");
    assert_eq!(
        Secret::RelaySubscription.user("ab"),
        "push_relay_subscription_ab"
    );
}

#[test]
fn old_files_with_extra_fields_still_read() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read(dir.path()).unwrap().servers.is_empty());

    fs::write(
            dir.path().join(FILE_NAME),
            r#"{"servers":[{"id":"a","url":"https://ci.example.com","label":"CI","paired_at":"2026-01-01T00:00:00Z"}]}"#,
        )
        .unwrap();
    assert_eq!(read(dir.path()).unwrap().servers[0].id, "a");

    remove_file(dir.path()).unwrap();
    remove_file(dir.path()).unwrap();
    assert!(read(dir.path()).unwrap().servers.is_empty());
}
