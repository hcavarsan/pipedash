use std::sync::Arc;

use super::*;
use crate::infrastructure::MemoryTokenStore;

fn config_in(data_dir: &Path) -> PipedashConfig {
    let mut config = PipedashConfig::default();
    config.storage.data_dir = data_dir.display().to_string();
    config
}

async fn locked_manager(config: &PipedashConfig) -> DomainResult<StorageManager> {
    StorageManager::with_token_store_locked(config.clone(), Arc::new(MemoryTokenStore::new()), true)
        .await
}

#[test]
fn corruption_errors_are_told_apart_from_bad_passwords() {
    assert!(is_database_corruption_error(
        "database disk image is malformed"
    ));
    assert!(is_database_corruption_error("File is not a database"));
    assert!(!is_database_corruption_error(
        "failed to decrypt vault password"
    ));
}

#[cfg(windows)]
fn hold(path: &Path) -> std::fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(path)
        .unwrap()
}

#[cfg(not(windows))]
fn hold(path: &Path) -> std::fs::File {
    std::fs::File::open(path).unwrap()
}

#[test]
fn a_database_still_held_open_briefly_is_backed_up_once_released() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("pipedash.db");
    std::fs::write(&db, b"not a database").unwrap();
    let held = hold(&db);
    let release = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        drop(held);
    });

    let backup = reset_corrupted_databases(dir.path()).unwrap();

    release.join().unwrap();
    assert!(!db.exists());
    assert!(backup.main_db_backup.is_some_and(|path| path.is_file()));
}

#[tokio::test]
async fn recovery_resets_the_corrupt_database_in_the_configured_data_dir() {
    for custom in [false, true] {
        let app_dir = tempfile::tempdir().unwrap();
        let custom_dir = tempfile::tempdir().unwrap();
        let data_dir = if custom {
            custom_dir.path()
        } else {
            app_dir.path()
        };
        let config = config_in(data_dir);
        std::fs::write(data_dir.join("pipedash.db"), vec![0x42u8; 8192]).unwrap();

        let result = build_with_recovery(&config, true, || locked_manager(&config)).await;
        assert!(
            result.is_ok(),
            "custom_data_dir={custom}: {:?}",
            result.err()
        );

        let backups = std::fs::read_dir(data_dir.join("corrupt_backups"))
            .unwrap()
            .count();
        assert_eq!(backups, 1, "custom_data_dir={custom}");
    }
}

#[tokio::test]
async fn corruption_is_not_recovered_when_the_caller_forbids_it() {
    let dir = tempfile::tempdir().unwrap();
    let config = config_in(dir.path());
    let db_path = dir.path().join("pipedash.db");
    std::fs::write(&db_path, vec![0x42u8; 8192]).unwrap();

    let result = build_with_recovery(&config, false, || locked_manager(&config)).await;

    assert!(matches!(result, Err(RecoveryFailure::Build(_))));
    assert!(db_path.exists());
    assert!(!dir.path().join("corrupt_backups").exists());
}

#[tokio::test]
async fn a_failed_retry_puts_the_original_databases_back() {
    let dir = tempfile::tempdir().unwrap();
    let config = config_in(dir.path());
    let db_path = dir.path().join("pipedash.db");
    std::fs::write(&db_path, b"stub original").unwrap();

    let result = build_with_recovery(&config, true, || async {
        Err(DomainError::DatabaseError("file is not a database".into()))
    })
    .await;

    assert!(matches!(result, Err(RecoveryFailure::Recovery(_))));
    assert_eq!(std::fs::read(&db_path).unwrap(), b"stub original");
}

#[tokio::test]
async fn other_failures_are_returned_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let config = config_in(dir.path());

    let result = build_with_recovery(&config, true, || async {
        Err(DomainError::AuthenticationFailed("wrong password".into()))
    })
    .await;

    assert!(matches!(
        result,
        Err(RecoveryFailure::Build(DomainError::AuthenticationFailed(_)))
    ));
}
