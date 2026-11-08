use std::sync::Arc;

use crate::event::EventBus;
use crate::infrastructure::migration::VAULT_PASSWORD_ENV_VAR;
use crate::infrastructure::{
    MigrationOptions,
    MigrationOrchestrator,
    MigrationPlan,
    MigrationResult,
    PipedashConfig,
    StorageManager,
    TokenStore,
};

fn keyring_target(
    target: &PipedashConfig, options: &MigrationOptions,
    external_token_store: Option<Arc<dyn TokenStore>>,
) -> Option<Arc<dyn TokenStore>> {
    let target_uses_encrypted =
        options.token_password.is_some() || std::env::var(VAULT_PASSWORD_ENV_VAR).is_ok();

    let store = external_token_store
        .filter(|_| target.storage.backend.is_sqlite() && !target_uses_encrypted);

    tracing::info!(
        "target_uses_encrypted={}, using_keyring_for_target={}",
        target_uses_encrypted,
        store.is_some()
    );
    store
}

pub async fn execute_migration(
    manager: &StorageManager, plan: MigrationPlan, options: MigrationOptions,
    event_bus: Option<Arc<dyn EventBus>>, external_token_store: Option<Arc<dyn TokenStore>>,
    is_desktop: bool,
) -> MigrationResult {
    let target_token_store = keyring_target(&plan.to, &options, external_token_store);
    MigrationOrchestrator::from_manager(manager, event_bus, target_token_store)
        .await
        .execute_migration(plan, options, is_desktop)
        .await
}

#[cfg(test)]
mod tests;
