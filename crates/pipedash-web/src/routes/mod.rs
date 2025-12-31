mod cache;
pub mod health;
mod metrics;
mod pipelines;
mod plugins;
mod preferences;
mod providers;
mod refresh;
mod setup;
mod storage;
mod system;
mod vault;

use axum::{
    routing::get,
    Router,
};

use crate::state::AppState;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health_check))
        .nest("/setup", setup::router())
        .nest("/providers", providers::router())
        .nest("/pipelines", pipelines::router())
        .nest("/plugins", plugins::router())
        .nest("/cache", cache::router())
        .nest("/metrics", metrics::router())
        .nest("/preferences", preferences::router())
        .nest("/refresh", refresh::router())
        .nest("/storage", storage::router())
        .nest("/vault", vault::router())
        .merge(system::router())
}
