use super::*;
use crate::infrastructure::{
    MemoryTokenStore,
    StorageBackendType,
};
use crate::test_env::EnvGuard;

fn external() -> Option<Arc<dyn TokenStore>> {
    Some(Arc::new(MemoryTokenStore::new()))
}

fn target(backend: StorageBackendType) -> PipedashConfig {
    let mut config = PipedashConfig::default();
    config.storage.backend = backend;
    config
}

fn options(token_password: Option<&str>) -> MigrationOptions {
    MigrationOptions {
        token_password: token_password.map(str::to_string),
        ..MigrationOptions::default()
    }
}

#[test]
fn the_external_token_store_is_reused_for_a_sqlite_target_without_a_password() {
    let _env = EnvGuard::set(&[(VAULT_PASSWORD_ENV_VAR, None)]);

    let store = keyring_target(
        &target(StorageBackendType::Sqlite),
        &options(None),
        external(),
    );

    assert!(store.is_some());
}

#[test]
fn an_encrypted_password_selection_drops_the_external_token_store() {
    {
        let _env = EnvGuard::set(&[(VAULT_PASSWORD_ENV_VAR, None)]);
        let store = keyring_target(
            &target(StorageBackendType::Sqlite),
            &options(Some("stub-password")),
            external(),
        );
        assert!(store.is_none());
    }

    let _env = EnvGuard::set(&[(VAULT_PASSWORD_ENV_VAR, Some("stub-password"))]);
    let store = keyring_target(
        &target(StorageBackendType::Sqlite),
        &options(None),
        external(),
    );
    assert!(store.is_none());
}

#[test]
fn a_postgres_target_never_uses_the_external_token_store() {
    let _env = EnvGuard::set(&[(VAULT_PASSWORD_ENV_VAR, None)]);

    let store = keyring_target(
        &target(StorageBackendType::Postgres),
        &options(None),
        external(),
    );

    assert!(store.is_none());
}

#[test]
fn no_external_token_store_stays_none() {
    let _env = EnvGuard::set(&[(VAULT_PASSWORD_ENV_VAR, None)]);

    let store = keyring_target(&target(StorageBackendType::Sqlite), &options(None), None);

    assert!(store.is_none());
}
