use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use pipedash_core::domain::{
    DomainError,
    DomainResult,
};
use pipedash_core::infrastructure::TokenStore;

pub struct FallbackTokenStore {
    primary: Arc<dyn TokenStore>,
    fallback: Arc<dyn TokenStore>,
}

impl FallbackTokenStore {
    pub fn new(primary: Arc<dyn TokenStore>, fallback: Arc<dyn TokenStore>) -> Self {
        Self { primary, fallback }
    }
}

#[async_trait]
impl TokenStore for FallbackTokenStore {
    async fn store_token(&self, provider_id: i64, token: &str) -> DomainResult<()> {
        self.primary.store_token(provider_id, token).await?;

        let _ = self.fallback.delete_token(provider_id).await;

        Ok(())
    }

    async fn get_token(&self, provider_id: i64) -> DomainResult<String> {
        let primary_error = match self.primary.get_token(provider_id).await {
            Ok(token) => {
                tracing::trace!(provider_id = provider_id, "Token found in primary store");
                return Ok(token);
            }
            Err(e) => {
                tracing::debug!(
                    provider_id = provider_id,
                    error = %e,
                    "Token not found in primary store, trying fallback"
                );
                e
            }
        };

        let fallback_error = match self.fallback.get_token(provider_id).await {
            Ok(token) => {
                tracing::info!(
                    provider_id = provider_id,
                    "Token found in fallback store (keyring), migrating to primary"
                );

                if let Err(e) = self.primary.store_token(provider_id, &token).await {
                    tracing::warn!(
                        provider_id = provider_id,
                        error = %e,
                        "Failed to migrate token to primary store (will retry on next access)"
                    );
                } else {
                    tracing::info!(
                        provider_id = provider_id,
                        "Token successfully migrated from keyring to encrypted storage"
                    );

                    if let Err(e) = self.fallback.delete_token(provider_id).await {
                        tracing::debug!(
                            provider_id = provider_id,
                            error = %e,
                            "Could not delete token from fallback after migration (non-critical)"
                        );
                    }
                }

                return Ok(token);
            }
            Err(e) => {
                tracing::debug!(
                    provider_id = provider_id,
                    error = %e,
                    "Token not found in fallback store either"
                );
                e
            }
        };

        let message = format!(
            "Token not found for provider {} (checked both encrypted storage and keyring)",
            provider_id
        );
        match (primary_error, fallback_error) {
            (DomainError::NotFound(_), DomainError::NotFound(_)) => {
                Err(DomainError::NotFound(message))
            }
            _ => Err(DomainError::DatabaseError(message)),
        }
    }

    async fn delete_token(&self, provider_id: i64) -> DomainResult<()> {
        let primary_result = self.primary.delete_token(provider_id).await;
        let fallback_result = self.fallback.delete_token(provider_id).await;

        primary_result?;
        fallback_result
    }

    async fn get_all_tokens(&self) -> DomainResult<HashMap<i64, String>> {
        let mut all_tokens = self.fallback.get_all_tokens().await?;
        let primary_tokens = self.primary.get_all_tokens().await?;

        all_tokens.extend(primary_tokens);

        Ok(all_tokens)
    }

    async fn export_encrypted(&self, password: &str) -> DomainResult<Vec<u8>> {
        let _all_tokens = self.get_all_tokens().await?;

        self.primary.export_encrypted(password).await
    }

    async fn import_encrypted(&self, data: &[u8], password: &str) -> DomainResult<()> {
        self.primary.import_encrypted(data, password).await
    }

    async fn warmup(&self) -> DomainResult<()> {
        self.primary.warmup().await
    }
}

#[cfg(test)]
mod tests {
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
}
