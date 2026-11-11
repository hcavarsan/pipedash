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
            DomainError::NotFound(format!("Token not found for provider {}", provider_id))
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

#[cfg(test)]
mod tests;
