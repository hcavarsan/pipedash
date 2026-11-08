use pipedash_core::infrastructure::MemoryTokenStore;

use super::*;

#[tokio::test]
async fn test_fallback_primary_first() {
    let primary = Arc::new(MemoryTokenStore::new());
    let fallback = Arc::new(MemoryTokenStore::new());

    primary.store_token(1, "primary-token").await.unwrap();

    let store = FallbackTokenStore::new(primary.clone(), fallback.clone());
    let token = store.get_token(1).await.unwrap();

    assert_eq!(token, "primary-token");
}

#[tokio::test]
async fn test_fallback_to_secondary() {
    let primary = Arc::new(MemoryTokenStore::new());
    let fallback = Arc::new(MemoryTokenStore::new());

    fallback.store_token(1, "fallback-token").await.unwrap();

    let store = FallbackTokenStore::new(primary.clone(), fallback.clone());
    let token = store.get_token(1).await.unwrap();

    assert_eq!(token, "fallback-token");

    let migrated = primary.get_token(1).await.unwrap();
    assert_eq!(migrated, "fallback-token");
}

#[tokio::test]
async fn test_store_goes_to_primary() {
    let primary = Arc::new(MemoryTokenStore::new());
    let fallback = Arc::new(MemoryTokenStore::new());

    let store = FallbackTokenStore::new(primary.clone(), fallback.clone());
    store.store_token(1, "new-token").await.unwrap();

    let token = primary.get_token(1).await.unwrap();
    assert_eq!(token, "new-token");

    assert!(fallback.get_token(1).await.is_err());
}

#[tokio::test]
async fn test_primary_overrides_fallback() {
    let primary = Arc::new(MemoryTokenStore::new());
    let fallback = Arc::new(MemoryTokenStore::new());

    primary.store_token(1, "primary-token").await.unwrap();
    fallback.store_token(1, "fallback-token").await.unwrap();

    let store = FallbackTokenStore::new(primary, fallback);
    let token = store.get_token(1).await.unwrap();

    assert_eq!(token, "primary-token");
}

struct LockedTokenStore;

#[async_trait]
impl TokenStore for LockedTokenStore {
    async fn store_token(&self, _provider_id: i64, _token: &str) -> DomainResult<()> {
        Ok(())
    }

    async fn get_token(&self, _provider_id: i64) -> DomainResult<String> {
        Err(DomainError::AuthenticationFailed(
            "vault is locked".to_string(),
        ))
    }

    async fn delete_token(&self, _provider_id: i64) -> DomainResult<()> {
        Err(DomainError::AuthenticationFailed("vault is locked".into()))
    }

    async fn get_all_tokens(&self) -> DomainResult<HashMap<i64, String>> {
        Err(DomainError::AuthenticationFailed("vault is locked".into()))
    }
}

#[tokio::test]
async fn test_missing_token_is_not_found_only_when_both_stores_lack_it() {
    let missing = FallbackTokenStore::new(
        Arc::new(MemoryTokenStore::new()),
        Arc::new(MemoryTokenStore::new()),
    );
    let locked = FallbackTokenStore::new(
        Arc::new(LockedTokenStore),
        Arc::new(MemoryTokenStore::new()),
    );

    assert!(matches!(
        missing.get_token(1).await,
        Err(DomainError::NotFound(_))
    ));
    assert!(matches!(
        locked.get_token(1).await,
        Err(DomainError::DatabaseError(_))
    ));
}

#[tokio::test]
async fn deletion_fails_when_either_store_cannot_delete_its_copy() {
    let primary_locked = FallbackTokenStore::new(
        Arc::new(LockedTokenStore),
        Arc::new(MemoryTokenStore::new()),
    );
    let fallback_locked = FallbackTokenStore::new(
        Arc::new(MemoryTokenStore::new()),
        Arc::new(LockedTokenStore),
    );

    assert!(primary_locked.delete_token(1).await.is_err());
    assert!(fallback_locked.delete_token(1).await.is_err());
    assert!(fallback_locked.get_all_tokens().await.is_err());
}
