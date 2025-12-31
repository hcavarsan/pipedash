use axum::{
    extract::State,
    routing::post,
    Json,
    Router,
};
use serde::Serialize;

use crate::error::{
    ApiResult,
    AppError,
};
use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct FactoryResetResponse {
    pub providers_removed: i64,
    pub caches_cleared: bool,
    pub tokens_cleared: bool,
    pub metrics_cleared: bool,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/factory-reset", post(factory_reset))
}

async fn factory_reset(State(state): State<AppState>) -> ApiResult<Json<FactoryResetResponse>> {
    let inner = state.inner.read().await;
    let core = inner.core.as_ref().ok_or_else(AppError::not_initialized)?;
    let providers = core.provider_service.list_providers().await?;
    let providers_count = providers.len() as i64;

    for provider in providers {
        core.provider_service.remove_provider(provider.id).await?;
    }

    if let Some(metrics_service) = &core.metrics_service {
        metrics_service.flush_metrics(None, true).await?;
    }

    Ok(Json(FactoryResetResponse {
        providers_removed: providers_count,
        caches_cleared: true,
        tokens_cleared: true,
        metrics_cleared: core.metrics_service.is_some(),
    }))
}
