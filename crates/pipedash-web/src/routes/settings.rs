use axum::{
    extract::State,
    http::StatusCode,
    routing::get,
    Json,
    Router,
};
use pipedash_core::infrastructure::{
    GeneralConfig,
    GeneralSettingsUpdate,
};

use crate::error::ApiResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/general",
        get(get_general_settings).put(update_general_settings),
    )
}

async fn get_general_settings(State(state): State<AppState>) -> ApiResult<Json<GeneralConfig>> {
    let config_state = state.config_state().await?;
    let settings = state
        .provider_service()
        .await?
        .general_settings(&config_state)
        .await;

    Ok(Json(settings))
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
