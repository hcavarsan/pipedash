use pipedash_core::infrastructure::config::ConfigLoader;
use pipedash_core::infrastructure::StorageManager;

use super::recovery::create_storage_manager;
use super::*;

const LAUNCH: [BootKind; 2] = [BootKind::Launch, BootKind::Resume];

fn facts(sqlite: bool, vault_password_set: bool, has_encrypted_tokens: bool) -> StorageFacts {
    StorageFacts {
        sqlite,
        vault_password_set,
        has_encrypted_tokens,
    }
}

#[test]
fn a_fresh_sqlite_install_without_a_password_uses_the_keyring() {
    for kind in LAUNCH {
        assert_eq!(
            choose_storage(kind, facts(true, false, false)),
            StorageChoice::Keyring
        );
    }
}

#[test]
fn encrypted_tokens_or_postgres_without_a_password_start_locked() {
    for kind in LAUNCH {
        assert_eq!(
            choose_storage(kind, facts(true, false, true)),
            StorageChoice::Locked
        );
        assert_eq!(
            choose_storage(kind, facts(false, false, false)),
            StorageChoice::Locked
        );
    }
}

#[test]
fn a_password_selects_the_encrypted_store() {
    for kind in LAUNCH {
        assert_eq!(
            choose_storage(kind, facts(true, true, false)),
            StorageChoice::EncryptedWithKeyringFallback
        );
        assert_eq!(
            choose_storage(kind, facts(true, true, true)),
            StorageChoice::EncryptedWithKeyringFallback
        );
        assert_eq!(
            choose_storage(kind, facts(false, true, false)),
            StorageChoice::FromConfig {
                desktop: false,
                allow_locked: true
            }
        );
    }
}

#[test]
fn setup_ignores_the_password_for_sqlite_and_keeps_postgres_unlockable() {
    assert_eq!(
        choose_storage(BootKind::Setup, facts(true, true, false)),
        StorageChoice::Keyring
    );
    assert_eq!(
        choose_storage(BootKind::Setup, facts(false, true, false)),
        StorageChoice::FromConfig {
            desktop: true,
            allow_locked: true
        }
    );
}

#[test]
fn unlock_is_always_strict() {
    for sqlite in [true, false] {
        assert_eq!(
            choose_storage(BootKind::Unlock, facts(sqlite, true, true)),
            StorageChoice::FromConfig {
                desktop: true,
                allow_locked: false
            }
        );
    }
}

#[test]
fn only_launch_like_boots_recover_and_only_unlocks_announce() {
    assert!(BootKind::Launch.recovers_corruption());
    assert!(BootKind::Resume.recovers_corruption());
    assert!(!BootKind::Setup.recovers_corruption());
    assert!(!BootKind::Unlock.recovers_corruption());
    assert!(BootKind::Unlock.announces_unlock());
    assert!(BootKind::Resume.announces_unlock());
    assert!(!BootKind::Launch.announces_unlock());
    assert!(!BootKind::Setup.announces_unlock());
}

#[test]
fn an_engaged_app_lock_only_boots_for_authenticated_callers() {
    assert!(boot_denied(true, true, false));
    assert!(!boot_denied(true, true, true));
    assert!(!boot_denied(true, false, false));
    assert!(!boot_denied(false, true, false));
}

async fn boot_with_corrupt_database(kind: BootKind) -> (Result<StorageManager, BootError>, usize) {
    let dir = tempfile::tempdir().unwrap();
    let mut config = PipedashConfig::default();
    config.storage.data_dir = dir.path().display().to_string();
    std::fs::write(dir.path().join("pipedash.db"), vec![0x42u8; 8192]).unwrap();

    let result = create_storage_manager(kind, StorageChoice::Locked, &config).await;
    let backups = std::fs::read_dir(dir.path().join("corrupt_backups"))
        .map(|entries| entries.count())
        .unwrap_or(0);

    (result, backups)
}

#[tokio::test]
async fn launch_boots_recover_a_corrupt_database() {
    for kind in LAUNCH {
        let (result, backups) = boot_with_corrupt_database(kind).await;

        assert!(result.is_ok(), "{kind:?}: {:?}", result.err());
        assert_eq!(backups, 1, "{kind:?}");
    }
}

#[tokio::test]
async fn setup_and_unlock_boots_report_corruption_instead_of_recovering() {
    let (setup, setup_backups) = boot_with_corrupt_database(BootKind::Setup).await;
    let (unlock, unlock_backups) = boot_with_corrupt_database(BootKind::Unlock).await;

    assert!(matches!(setup, Err(BootError::Storage(_))));
    assert!(matches!(unlock, Err(BootError::InvalidPassword(_))));
    assert_eq!((setup_backups, unlock_backups), (0, 0));
}

#[tokio::test]
async fn a_config_that_fails_to_load_keeps_the_launch_going_for_the_error_screen() {
    for content in [
        "[storage\nbackend = \"sqlite\"\n",
        "[storage]\ndata_dir = \"${PIPEDASH_TEST_UNSET_VAR_190}\"\n",
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("config.toml"), content).unwrap();

        let launch = launch_outcome(load_config(dir.path()).await);
        assert!(launch.is_ok(), "{content}: {launch:?}");

        let status =
            ConfigLoader::get_setup_status(dir.path(), &crate::plugin_registry::plugin_registry());
        assert!(status.config_exists && !status.config_valid, "{content}");
        assert!(!status.validation_errors.is_empty(), "{content}");
    }
}

#[test]
fn a_storage_failure_still_stops_the_launch() {
    let launch = launch_outcome::<()>(Err(BootError::Storage("connection refused".into())));
    assert!(matches!(launch, Err(BootError::Storage(_))));
}
