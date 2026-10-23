use axum::{
    extract::State,
    routing::post,
    Json,
    Router,
};
use pipedash_core::domain::RefreshResult;

use crate::error::ApiResult;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/all", post(refresh_all))
}

async fn refresh_all(State(state): State<AppState>) -> ApiResult<Json<Vec<RefreshResult>>> {
    let pipeline_service = state.pipeline_service().await?;
    Ok(Json(pipeline_service.refresh_all().await?))
}
