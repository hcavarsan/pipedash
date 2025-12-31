use axum::{
    extract::State,
    Json,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub database: DatabaseHealth,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub setup_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseHealth {
    pub connected: bool,
    pub provider_count: usize,
}

pub async fn health_check(State(state): State<AppState>) -> Json<HealthResponse> {
    let inner = state.inner.read().await;

    if inner.setup_required || inner.config_error.is_some() {
        return Json(HealthResponse {
            status: if inner.setup_required {
                "setup_required".to_string()
            } else {
                "config_error".to_string()
            },
            version: env!("CARGO_PKG_VERSION").to_string(),
            database: DatabaseHealth {
                connected: false,
                provider_count: 0,
            },
            setup_required: inner.setup_required,
            config_error: inner.config_error.clone(),
        });
    }

    let core = inner.core.as_ref().unwrap();

    if !inner.token_store_ready {
        return Json(HealthResponse {
            status: "initializing".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            database: DatabaseHealth {
                connected: false,
                provider_count: 0,
            },
            setup_required: false,
            config_error: None,
        });
    }

    let provider_count = core
        .provider_service
        .list_providers()
        .await
        .map(|providers: Vec<pipedash_core::ProviderSummary>| providers.len())
        .unwrap_or(0);

    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        database: DatabaseHealth {
            connected: true,
            provider_count,
        },
        setup_required: false,
        config_error: None,
    })
}
