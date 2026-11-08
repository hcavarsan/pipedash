use keyring_core::mock::Cred;

use super::*;

fn store() -> KeyringTokenStore {
    KeyringTokenStore {
        entries: Arc::new(
            ["tokens", "provider_1", "provider_2"]
                .into_iter()
                .map(|user| {
                    (
                        user.to_string(),
                        Entry {
                            inner: keyring_core::Entry::new_with_credential(Arc::new(Cred {
                                specifiers: ("pipedash".into(), user.into()),
                                inner: Default::default(),
                            })),
                        },
                    )
                })
                .collect(),
        ),
        ..KeyringTokenStore::new()
    }
}

fn fail_next(store: &KeyringTokenStore, user: &str) {
    store.entries[user]
        .inner
        .as_any()
        .downcast_ref::<Cred>()
        .unwrap()
        .set_error(KeyringError::Invalid(
            "locked".into(),
            "access denied".into(),
        ));
}

#[tokio::test]
async fn deleting_the_last_token_does_not_resurrect_it_when_adding_another() {
    let store = store();
    store.store_token(1, "old-token").await.unwrap();
    store.entries["provider_1"]
        .set_password("legacy-token")
        .unwrap();

    store.delete_token(1).await.unwrap();

    assert!(store.get_all_tokens().await.unwrap().is_empty());
    assert!(matches!(
        store.entries["tokens"].get_password(),
        Err(KeyringError::NoEntry)
    ));
    assert!(matches!(
        store.get_token(1).await,
        Err(DomainError::NotFound(_))
    ));
    store.store_token(2, "new-token").await.unwrap();
    *store.token_cache.lock().await = None;
    assert_eq!(
        store.get_all_tokens().await.unwrap(),
        HashMap::from([(2, "new-token".into())])
    );
}

#[tokio::test]
async fn deleting_an_absent_token_is_idempotent() {
    let store = store();
    store.delete_token(1).await.unwrap();
    store.delete_token(1).await.unwrap();
    assert!(store.get_all_tokens().await.unwrap().is_empty());
}

#[tokio::test]
async fn failed_persistent_deletion_keeps_the_cache_and_supports_retry() {
    let store = store();
    store.store_token(1, "old-token").await.unwrap();
    fail_next(&store, "tokens");

    assert!(store.delete_token(1).await.is_err());
    assert_eq!(store.get_token(1).await.unwrap(), "old-token");
    assert!(store.entries["tokens"].get_password().is_ok());

    store.delete_token(1).await.unwrap();
    assert!(store.get_all_tokens().await.unwrap().is_empty());
}

#[tokio::test]
async fn failed_legacy_deletion_does_not_remove_the_current_token() {
    let store = store();
    store.store_token(1, "current-token").await.unwrap();
    store.entries["provider_1"]
        .set_password("legacy-token")
        .unwrap();
    fail_next(&store, "provider_1");

    assert!(store.delete_token(1).await.is_err());
    assert_eq!(store.get_token(1).await.unwrap(), "current-token");
    assert_eq!(
        store.entries["provider_1"].get_password().unwrap(),
        "legacy-token"
    );

    store.delete_token(1).await.unwrap();
    assert!(matches!(
        store.get_token(1).await,
        Err(DomainError::NotFound(_))
    ));
}
