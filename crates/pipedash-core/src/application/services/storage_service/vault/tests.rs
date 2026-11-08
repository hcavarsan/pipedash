use super::*;
use crate::infrastructure::StorageBackendType;

#[test]
fn a_password_free_session_is_ready_without_an_environment_password() {
    assert!(vault_is_unlocked(false, false, false));
}

#[test]
fn an_encrypted_session_requires_an_environment_password() {
    assert!(!vault_is_unlocked(true, false, false));
    assert!(vault_is_unlocked(true, true, false));
}

#[test]
fn app_lock_revokes_readiness_regardless_of_password_storage() {
    for requires_password in [false, true] {
        for password_set in [false, true] {
            assert!(!vault_is_unlocked(requires_password, password_set, true));
        }
    }
}

#[tokio::test]
async fn a_fresh_sqlite_store_needs_no_vault_password() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = PipedashConfig::default();
    config.storage.backend = StorageBackendType::Sqlite;
    config.storage.data_dir = dir.path().display().to_string();

    assert!(!requires_vault_password(&config).await);
}

#[tokio::test]
async fn a_postgres_store_always_needs_the_vault_password() {
    let mut config = PipedashConfig::default();
    config.storage.backend = StorageBackendType::Postgres;

    assert!(requires_vault_password(&config).await);
}

#[test]
fn unlock_responses_only_carry_a_reason_when_cancelled() {
    assert_eq!(
        serde_json::to_value(UnlockVaultResponse::cancelled()).unwrap(),
        serde_json::json!({ "success": false, "message": "cancelled", "reason": "cancelled" })
    );
    assert_eq!(
        serde_json::to_value(UnlockVaultResponse::ok("done")).unwrap(),
        serde_json::json!({ "success": true, "message": "done" })
    );
}

#[test]
fn password_sources_use_the_wire_names() {
    let names: Vec<_> = [
        PasswordSource::EnvVar,
        PasswordSource::Keyring,
        PasswordSource::Session,
        PasswordSource::None,
    ]
    .iter()
    .map(|source| serde_json::to_value(source).unwrap())
    .collect();

    assert_eq!(
        names,
        [
            serde_json::json!("env_var"),
            serde_json::json!("keyring"),
            serde_json::json!("session"),
            serde_json::json!("none"),
        ]
    );
}
