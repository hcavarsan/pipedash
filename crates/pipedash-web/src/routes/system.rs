use axum::{
    extract::State,
    routing::post,
    Json,
    Router,
};
use serde::Serialize;

use crate::error::ApiResult;
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
    let core = state.core().await?;
    let providers_removed = core.factory_reset().await?;

    Ok(Json(FactoryResetResponse {
        providers_removed: providers_removed as i64,
        caches_cleared: true,
        tokens_cleared: true,
        metrics_cleared: true,
    }))
}
