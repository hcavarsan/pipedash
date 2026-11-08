use std::collections::HashMap;

use async_trait::async_trait;

use super::*;
use crate::application::test_support::plugins::registry;
use crate::event::NoOpEventBus;
use crate::infrastructure::{
    MemoryTokenStore,
    PipedashConfig,
    TokenStore,
};

struct StubWarmupFailure;

#[async_trait]
impl TokenStore for StubWarmupFailure {
    async fn store_token(&self, _provider_id: i64, _token: &str) -> DomainResult<()> {
        Ok(())
    }

    async fn get_token(&self, provider_id: i64) -> DomainResult<String> {
        Err(DomainError::NotFound(provider_id.to_string()))
    }

    async fn delete_token(&self, _provider_id: i64) -> DomainResult<()> {
        Ok(())
    }

    async fn get_all_tokens(&self) -> DomainResult<HashMap<i64, String>> {
        Ok(HashMap::new())
    }

    async fn warmup(&self) -> DomainResult<()> {
        Err(DomainError::InternalError(
            "stub keyring unavailable".into(),
        ))
    }
}

fn config(dir: &std::path::Path) -> PipedashConfig {
    let mut config = PipedashConfig::default();
    config.storage.data_dir = dir.display().to_string();
    config.general.metrics_enabled = false;
    config
}

async fn prepared(
    dir: &std::path::Path, tokens: Arc<dyn TokenStore>, locked: bool,
) -> PreparedSession {
    let manager = if locked {
        StorageManager::with_token_store_locked(config(dir), tokens, true).await
    } else {
        StorageManager::with_token_store(config(dir), tokens, true).await
    }
    .unwrap();

    prepare_session(manager, Arc::new(NoOpEventBus), registry())
        .await
        .unwrap()
}

#[tokio::test]
async fn warming_an_unlocked_session_starts_the_background_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let session = prepared(dir.path(), Arc::new(MemoryTokenStore::new()), false).await;

    assert!(!session.vault_locked);
    assert!(session.warm().await.unwrap());
    session.core.shutdown().await;
}

#[tokio::test]
async fn warming_a_locked_session_leaves_the_token_store_alone() {
    let dir = tempfile::tempdir().unwrap();
    let session = prepared(dir.path(), Arc::new(StubWarmupFailure), true).await;

    assert!(session.vault_locked);
    assert!(!session.warm().await.unwrap());
    session.core.shutdown().await;
}

#[tokio::test]
async fn a_failed_warmup_is_an_error_and_starts_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let session = prepared(dir.path(), Arc::new(StubWarmupFailure), false).await;

    let error = session.warm().await.unwrap_err();

    assert!(error.to_string().contains("stub keyring unavailable"));
    session.core.shutdown().await;
}
