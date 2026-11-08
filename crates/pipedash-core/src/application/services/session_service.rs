use std::sync::Arc;

use pipedash_plugin_api::PluginRegistry;

use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::event::EventBus;
use crate::infrastructure::StorageManager;
use crate::CoreContext;

pub struct PreparedSession {
    pub core: CoreContext,
    pub storage_manager: StorageManager,
    pub vault_locked: bool,
}

pub async fn prepare_session(
    storage_manager: StorageManager, event_bus: Arc<dyn EventBus>,
    plugin_registry: Arc<PluginRegistry>,
) -> DomainResult<PreparedSession> {
    let core = CoreContext::with_storage_manager(&storage_manager, event_bus, plugin_registry)
        .await
        .map_err(|e| DomainError::InternalError(e.to_string()))?;
    let vault_locked = storage_manager.is_vault_locked().await;

    Ok(PreparedSession {
        core,
        storage_manager,
        vault_locked,
    })
}

impl PreparedSession {
    pub async fn warm(&self) -> DomainResult<bool> {
        if self.vault_locked {
            return Ok(false);
        }
        self.core
            .warmup_token_store()
            .await
            .map_err(|e| DomainError::InternalError(e.to_string()))?;
        self.core.start_background_tasks().await;

        Ok(true)
    }
}

#[cfg(test)]
mod tests;
