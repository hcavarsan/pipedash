use axum::{
    extract::State,
    http::StatusCode,
    routing::put,
    Json,
    Router,
};
use pipedash_core::infrastructure::GeneralSettingsUpdate;

use crate::error::ApiResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/general", put(update_general_settings))
}

async fn update_general_settings(
    State(state): State<AppState>, Json(update): Json<GeneralSettingsUpdate>,
) -> ApiResult<StatusCode> {
    let config_state = state.config_state().await?;
    state
        .provider_service()
        .await?
        .update_general_settings(&config_state, update)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests;
