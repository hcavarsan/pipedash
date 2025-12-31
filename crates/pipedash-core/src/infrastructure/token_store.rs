use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;

use crate::domain::{
    DomainError,
    DomainResult,
};

#[async_trait]
pub trait TokenStore: Send + Sync {
    async fn store_token(&self, provider_id: i64, token: &str) -> DomainResult<()>;

    async fn get_token(&self, provider_id: i64) -> DomainResult<String>;

    async fn delete_token(&self, provider_id: i64) -> DomainResult<()>;

    async fn get_all_tokens(&self) -> DomainResult<HashMap<i64, String>>;

    async fn get_token_by_name(&self, _name: &str) -> DomainResult<String> {
        Err(DomainError::InternalError(
            "This token store does not support name-based lookups".into(),
        ))
    }

    async fn export_encrypted(&self, _password: &str) -> DomainResult<Vec<u8>> {
        Err(DomainError::InternalError(
            "This token store does not support export".into(),
        ))
    }

    async fn import_encrypted(&self, _data: &[u8], _password: &str) -> DomainResult<()> {
        Err(DomainError::InternalError(
            "This token store does not support import".into(),
        ))
    }

    async fn warmup(&self) -> DomainResult<()> {
        Ok(())
    }
}

pub struct MemoryTokenStore {
    tokens: RwLock<HashMap<i64, String>>,
}

impl MemoryTokenStore {
    pub fn new() -> Self {
        Self {
            tokens: RwLock::new(HashMap::new()),
        }
    }

    pub fn with_tokens(tokens: HashMap<i64, String>) -> Self {
        Self {
            tokens: RwLock::new(tokens),
        }
    }
}

impl Default for MemoryTokenStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TokenStore for MemoryTokenStore {
    async fn store_token(&self, provider_id: i64, token: &str) -> DomainResult<()> {
        let mut tokens = self
            .tokens
            .write()
            .map_err(|e| DomainError::InternalError(format!("Lock poisoned: {}", e)))?;
        tokens.insert(provider_id, token.to_string());
        Ok(())
    }

    async fn get_token(&self, provider_id: i64) -> DomainResult<String> {
        let tokens = self
            .tokens
            .read()
            .map_err(|e| DomainError::InternalError(format!("Lock poisoned: {}", e)))?;
        tokens.get(&provider_id).cloned().ok_or_else(|| {
            DomainError::DatabaseError(format!("Token not found for provider {}", provider_id))
        })
    }

    async fn delete_token(&self, provider_id: i64) -> DomainResult<()> {
        let mut tokens = self
            .tokens
            .write()
            .map_err(|e| DomainError::InternalError(format!("Lock poisoned: {}", e)))?;
        tokens.remove(&provider_id);
        Ok(())
    }

    async fn get_all_tokens(&self) -> DomainResult<HashMap<i64, String>> {
        let tokens = self
            .tokens
            .read()
            .map_err(|e| DomainError::InternalError(format!("Lock poisoned: {}", e)))?;
        Ok(tokens.clone())
    }
}

pub struct EnvTokenStore {
    prefix: String,
    cache: RwLock<HashMap<i64, String>>,
}

impl EnvTokenStore {
    pub fn new() -> Self {
        Self::with_prefix("PIPEDASH_TOKEN_")
    }

    pub fn with_prefix(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            cache: RwLock::new(HashMap::new()),
        }
    }

    fn env_var_name(&self, provider_id: i64) -> String {
        format!("{}{}", self.prefix, provider_id)
    }
}

impl Default for EnvTokenStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TokenStore for EnvTokenStore {
    async fn store_token(&self, provider_id: i64, token: &str) -> DomainResult<()> {
        let mut cache = self
            .cache
            .write()
            .map_err(|e| DomainError::InternalError(format!("Lock poisoned: {}", e)))?;
        cache.insert(provider_id, token.to_string());
        Ok(())
    }

    async fn get_token(&self, provider_id: i64) -> DomainResult<String> {
        {
            let cache = self
                .cache
                .read()
                .map_err(|e| DomainError::InternalError(format!("Lock poisoned: {}", e)))?;
            if let Some(token) = cache.get(&provider_id) {
                return Ok(token.clone());
            }
        }

        let var_name = self.env_var_name(provider_id);
        std::env::var(&var_name).map_err(|_| {
            DomainError::DatabaseError(format!(
                "Token not found for provider {} (env var: {})",
                provider_id, var_name
            ))
        })
    }

    async fn delete_token(&self, provider_id: i64) -> DomainResult<()> {
        let mut cache = self
            .cache
            .write()
            .map_err(|e| DomainError::InternalError(format!("Lock poisoned: {}", e)))?;
        cache.remove(&provider_id);
        Ok(())
    }

    async fn get_all_tokens(&self) -> DomainResult<HashMap<i64, String>> {
        let cache = self
            .cache
            .read()
            .map_err(|e| DomainError::InternalError(format!("Lock poisoned: {}", e)))?;
        Ok(cache.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_store() {
        let store = MemoryTokenStore::new();

        store.store_token(1, "secret_token").await.unwrap();

        let token = store.get_token(1).await.unwrap();
        assert_eq!(token, "secret_token");

        let all = store.get_all_tokens().await.unwrap();
        assert_eq!(all.len(), 1);

        store.delete_token(1).await.unwrap();

        assert!(store.get_token(1).await.is_err());
    }

    #[tokio::test]
    async fn test_memory_store_with_initial_tokens() {
        let mut initial = HashMap::new();
        initial.insert(1, "token1".to_string());
        initial.insert(2, "token2".to_string());

        let store = MemoryTokenStore::with_tokens(initial);

        assert_eq!(store.get_token(1).await.unwrap(), "token1");
        assert_eq!(store.get_token(2).await.unwrap(), "token2");
    }
}
