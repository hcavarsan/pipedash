mod alerts;
mod cache;
mod devices;
pub mod health;
mod metrics;
mod notifications;
mod pipeline_views;
mod pipelines;
mod plugins;
mod preferences;
mod providers;
mod push;
mod refresh;
mod run_logs;
mod settings;
mod setup;
mod sign_in;
mod storage;
mod system;
mod vault;

#[cfg(test)]
pub(crate) static CONFIG_ENV: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

use axum::{
    routing::get,
    Router,
};

use crate::state::AppState;

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health_check))
        .nest("/alerts", alerts::router())
        .nest("/setup", setup::router())
        .nest("/providers", providers::router())
        .nest("/pipelines", pipelines::router().merge(run_logs::router()))
        .nest("/pipeline-views", pipeline_views::router())
        .nest("/plugins", plugins::router())
        .nest("/cache", cache::router())
        .nest("/devices", devices::router())
        .nest("/metrics", metrics::router())
        .nest("/notifications", notifications::router())
        .nest("/preferences", preferences::router())
        .nest("/refresh", refresh::router())
        .nest("/settings", settings::router())
        .nest("/push", push::router())
        .nest("/sign-in", sign_in::router())
        .nest("/storage", storage::router())
        .nest("/vault", vault::router())
        .route("/downloads/{ticket}", get(run_logs::download))
        .merge(system::router())
}

#[cfg(test)]
mod tests;
