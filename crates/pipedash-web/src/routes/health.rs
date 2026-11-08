use axum::{
    extract::State,
    http::StatusCode,
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

pub async fn health_check(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let inner = state.inner.read().await;

    if inner.setup_required || inner.config_error.is_some() {
        return (
            StatusCode::OK,
            Json(HealthResponse {
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
            }),
        );
    }

    let Some(core) = inner.core.as_ref().filter(|_| inner.token_store_ready) else {
        return (
            StatusCode::OK,
            Json(HealthResponse {
                status: "initializing".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                database: DatabaseHealth {
                    connected: false,
                    provider_count: 0,
                },
                setup_required: false,
                config_error: None,
            }),
        );
    };

    let (status, provider_count) = match core.provider_service.list_providers().await {
        Ok(providers) => (StatusCode::OK, Some(providers.len())),
        Err(e) => {
            tracing::warn!(error = %e, "Health check database query failed");
            (StatusCode::SERVICE_UNAVAILABLE, None)
        }
    };

    (
        status,
        Json(HealthResponse {
            status: if provider_count.is_some() {
                "ok".to_string()
            } else {
                "database_error".to_string()
            },
            version: env!("CARGO_PKG_VERSION").to_string(),
            database: DatabaseHealth {
                connected: provider_count.is_some(),
                provider_count: provider_count.unwrap_or(0),
            },
            setup_required: false,
            config_error: None,
        }),
    )
}

#[cfg(test)]
mod tests;
