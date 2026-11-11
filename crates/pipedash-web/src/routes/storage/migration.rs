use std::sync::Arc;

use axum::{
    extract::State,
    Json,
};
use pipedash_core::application::services::storage_service::migration as storage_migration;
use pipedash_core::application::services::storage_service::{
    self,
    TestConnectionResult,
};
use pipedash_core::event::EventBus;
use pipedash_core::infrastructure::migration::VAULT_PASSWORD_ENV_VAR;
use pipedash_core::infrastructure::{
    MigrationOptions,
    MigrationPlan,
    MigrationResult,
    PipedashConfig,
    StorageManager,
};
use serde::Deserialize;

use super::session::reload_storage;
use crate::error::{
    ApiResult,
    AppError,
};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct ExecuteMigrationRequest {
    pub plan: MigrationPlan,
    pub options: MigrationOptions,
}

async fn session_handles(state: &AppState) -> ApiResult<(Arc<StorageManager>, Arc<dyn EventBus>)> {
    let storage_manager = state
        .storage_manager()
        .await
        .ok_or_else(|| AppError::internal("Storage manager not available"))?;
    let core = state.core().await?;

    Ok((storage_manager, core.event_bus.clone()))
}

pub(super) async fn test_storage_connection(
    Json(config): Json<PipedashConfig>,
) -> ApiResult<Json<TestConnectionResult>> {
    Ok(Json(
        storage_service::test_connection(config, None, false).await?,
    ))
}

pub(super) async fn execute_migration(
    State(state): State<AppState>, Json(req): Json<ExecuteMigrationRequest>,
) -> ApiResult<Json<MigrationResult>> {
    let _gate = state.setup_lock.lock().await;
    let token_password = req.options.token_password.clone();

    let (manager, event_bus) = session_handles(&state).await?;
    let result = storage_migration::execute_migration(
        &manager,
        req.plan,
        req.options,
        Some(event_bus),
        None,
        false,
    )
    .await;

    if result.success {
        if let Some(password) = &token_password {
            std::env::set_var(VAULT_PASSWORD_ENV_VAR, password);
            state.set_api_token(password);
            tracing::info!("Vault password set in environment for this session");
        }

        if let Err(e) = reload_storage(&state).await {
            tracing::error!("{} after migration. Manual restart required.", e);
        }
    }

    Ok(Json(result))
}
