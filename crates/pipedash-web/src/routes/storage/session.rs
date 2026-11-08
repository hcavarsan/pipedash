use std::sync::Arc;

use pipedash_core::application::services::session_service::{
    prepare_session,
    PreparedSession,
};
use pipedash_core::domain::{
    DomainError,
    DomainResult,
};
use pipedash_core::infrastructure::{
    ConfigLoader,
    Platform,
    StorageManager,
};

use crate::state::AppState;

pub(crate) async fn install_session(
    state: &AppState, session: PreparedSession,
) -> DomainResult<()> {
    if let Err(error) = session.warm().await {
        session.core.shutdown().await;
        return Err(error);
    }

    let PreparedSession {
        core,
        storage_manager,
        vault_locked,
    } = session;
    let old_core = {
        let mut inner = state.inner.write().await;
        inner.storage_manager = Some(Arc::new(storage_manager));
        inner.token_store_ready = !vault_locked;
        inner.core.replace(core)
    };
    if let Some(old_core) = old_core {
        state.download_tickets.clear();
        old_core.shutdown().await;
    }

    Ok(())
}

pub(crate) async fn reload_storage(state: &AppState) -> DomainResult<()> {
    let config = tokio::task::spawn_blocking(|| {
        ConfigLoader::load_or_create(&ConfigLoader::discover_config_path(), Platform::Server)
    })
    .await
    .map_err(|e| DomainError::InternalError(format!("Failed to reload config: {e}")))?
    .map_err(|e| DomainError::InternalError(format!("Failed to reload config: {e}")))?;
    tracing::info!(
        "Reloading storage manager with new config: {}",
        config.storage.summary()
    );

    let storage_manager = StorageManager::from_config(config, false)
        .await
        .map_err(|e| {
            DomainError::InternalError(format!("Failed to create new storage manager: {e}"))
        })?;
    let session = prepare_session(
        storage_manager,
        state.ws_event_bus.clone(),
        Arc::clone(&state.plugin_registry),
    )
    .await
    .map_err(|e| DomainError::InternalError(format!("Failed to recreate CoreContext: {e}")))?;
    install_session(state, session).await?;
    tracing::info!("Storage reloaded - token store warmed up and background tasks started");

    Ok(())
}
