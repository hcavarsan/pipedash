use std::collections::HashMap;

use axum::{
    extract::{
        Path,
        State,
    },
    routing::{
        get,
        post,
    },
    Json,
    Router,
};
use pipedash_core::domain::{
    SignInStart,
    SignInStatus,
};
use serde::Deserialize;

use crate::error::ApiResult;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct StartRequest {
    provider_type: String,
    #[serde(default)]
    config: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct CompleteRequest {
    redirect_url: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(start_sign_in))
        .route("/{id}", get(sign_in_status).delete(cancel_sign_in))
        .route("/{id}/complete", post(complete_sign_in))
}

async fn start_sign_in(
    State(state): State<AppState>, Json(req): Json<StartRequest>,
) -> ApiResult<Json<SignInStart>> {
    let start = state
        .provider_service()
        .await?
        .start_sign_in(&req.provider_type, req.config)
        .await?;
    Ok(Json(SignInStart {
        manual: true,
        ..start
    }))
}

async fn sign_in_status(
    State(state): State<AppState>, Path(id): Path<String>,
) -> ApiResult<Json<SignInStatus>> {
    Ok(Json(
        state.provider_service().await?.sign_in_status(&id).await?,
    ))
}

async fn complete_sign_in(
    State(state): State<AppState>, Path(id): Path<String>, Json(req): Json<CompleteRequest>,
) -> ApiResult<Json<SignInStatus>> {
    Ok(Json(
        state
            .provider_service()
            .await?
            .complete_sign_in(&id, &req.redirect_url)
            .await?,
    ))
}

async fn cancel_sign_in(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<()> {
    state.provider_service().await?.cancel_sign_in(&id).await;
    Ok(())
}
