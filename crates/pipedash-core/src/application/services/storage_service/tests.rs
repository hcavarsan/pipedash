use super::*;
use crate::infrastructure::MemoryTokenStore;

fn failure(message: &str) -> Result<(), String> {
    Err(message.to_string())
}

#[test]
fn every_backend_outcome_has_its_own_message() {
    let cases = [
        (Ok(()), Ok(()), true, "All storage backends are accessible"),
        (
            Ok(()),
            failure("Cache backend: gone"),
            false,
            "Config backend OK, but cache backend failed: Cache backend: gone",
        ),
        (
            failure("Config backend: down"),
            Ok(()),
            false,
            "Cache backend OK, but config backend failed: Config backend: down",
        ),
        (
            failure("Config backend: down"),
            failure("Cache backend: gone"),
            false,
            "Both backends failed - Config: Config backend: down, Cache: Cache backend: gone",
        ),
    ];

    for (config_test, cache_test, success, message) in cases {
        let outcome = connection_outcome(config_test, cache_test);
        assert_eq!(outcome.success, success, "{message}");
        assert_eq!(outcome.message, message);
    }
}

#[tokio::test]
async fn a_healthy_store_passes_both_backend_checks() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = PipedashConfig::default();
    config.storage.data_dir = dir.path().display().to_string();

    let outcome = test_connection(config, Some(Arc::new(MemoryTokenStore::new())), true)
        .await
        .unwrap();

    assert!(outcome.success, "{}", outcome.message);
    assert_eq!(outcome.message, "All storage backends are accessible");
}

#[tokio::test]
async fn a_store_that_cannot_open_is_reported_with_its_cause() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, b"file").unwrap();
    let mut config = PipedashConfig::default();
    config.storage.data_dir = blocker.join("data").display().to_string();

    let error = test_connection(config, Some(Arc::new(MemoryTokenStore::new())), true)
        .await
        .unwrap_err();

    assert!(error.to_string().contains("Failed to initialize storage"));
}
