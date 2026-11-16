use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use super::ReloadShadow;
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::TokenStore;

pub(crate) struct ShadowedTokens {
    inner: Arc<dyn TokenStore>,
    shadow: Arc<ReloadShadow>,
}

impl ShadowedTokens {
    pub(crate) fn new(inner: Arc<dyn TokenStore>, shadow: Arc<ReloadShadow>) -> Self {
        Self { inner, shadow }
    }
}

#[async_trait]
impl TokenStore for ShadowedTokens {
    async fn store_token(&self, provider_id: i64, token: &str) -> DomainResult<()> {
        self.inner.store_token(provider_id, token).await
    }

    async fn get_token(&self, provider_id: i64) -> DomainResult<String> {
        match self.shadow.token(provider_id) {
            Some(Some(token)) => Ok(token),
            Some(None) => Err(DomainError::NotFound(format!(
                "Token not found for provider {provider_id}"
            ))),
            None => self.inner.get_token(provider_id).await,
        }
    }

    async fn delete_token(&self, provider_id: i64) -> DomainResult<()> {
        self.inner.delete_token(provider_id).await
    }

    async fn get_all_tokens(&self) -> DomainResult<HashMap<i64, String>> {
        Ok(self.shadow.tokens(self.inner.get_all_tokens().await?))
    }

    async fn get_token_by_name(&self, name: &str) -> DomainResult<String> {
        self.inner.get_token_by_name(name).await
    }

    async fn export_encrypted(&self, password: &str) -> DomainResult<Vec<u8>> {
        self.inner.export_encrypted(password).await
    }

    async fn import_encrypted(&self, data: &[u8], password: &str) -> DomainResult<()> {
        self.inner.import_encrypted(data, password).await
    }

    async fn warmup(&self) -> DomainResult<()> {
        self.inner.warmup().await
    }
}
