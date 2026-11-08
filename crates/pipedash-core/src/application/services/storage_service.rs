pub mod config;
pub mod migration;
pub mod recovery;
pub mod vault;

use std::sync::Arc;

use serde::Serialize;

use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::{
    PipedashConfig,
    StorageManager,
    TokenStore,
};

#[derive(Debug, Serialize)]
pub struct TestConnectionResult {
    pub success: bool,
    pub message: String,
}

pub async fn open_manager(
    config: PipedashConfig, token_store: Option<Arc<dyn TokenStore>>, is_desktop: bool,
) -> DomainResult<StorageManager> {
    match token_store {
        Some(token_store) => {
            StorageManager::with_token_store(config, token_store, is_desktop).await
        }
        None => StorageManager::from_config_allow_locked(config, is_desktop).await,
    }
}

pub async fn test_connection(
    config: PipedashConfig, token_store: Option<Arc<dyn TokenStore>>, is_desktop: bool,
) -> DomainResult<TestConnectionResult> {
    let manager = open_manager(config, token_store, is_desktop)
        .await
        .map_err(|e| DomainError::InvalidConfig(format!("Failed to initialize storage: {}", e)))?;

    let config_test = manager
        .config_backend()
        .list_providers()
        .await
        .map(|_| ())
        .map_err(|e| format!("Config backend: {}", e));

    let cache_test = if manager.cache_backend().is_available().await {
        Ok(())
    } else {
        Err("Cache backend: Storage not available or not accessible".to_string())
    };

    Ok(connection_outcome(config_test, cache_test))
}

fn connection_outcome(
    config_test: Result<(), String>, cache_test: Result<(), String>,
) -> TestConnectionResult {
    let (success, message) = match (config_test, cache_test) {
        (Ok(()), Ok(())) => (true, "All storage backends are accessible".to_string()),
        (Ok(()), Err(cache_err)) => (
            false,
            format!("Config backend OK, but cache backend failed: {}", cache_err),
        ),
        (Err(config_err), Ok(())) => (
            false,
            format!(
                "Cache backend OK, but config backend failed: {}",
                config_err
            ),
        ),
        (Err(config_err), Err(cache_err)) => (
            false,
            format!(
                "Both backends failed - Config: {}, Cache: {}",
                config_err, cache_err
            ),
        ),
    };

    TestConnectionResult { success, message }
}

async fn blocking<T: Send + 'static>(job: impl FnOnce() -> T + Send + 'static) -> DomainResult<T> {
    tokio::task::spawn_blocking(job)
        .await
        .map_err(|e| DomainError::InternalError(format!("Background task failed: {e}")))
}

#[cfg(test)]
mod tests;
