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

    let core = inner.core.as_ref().unwrap();

    if !inner.token_store_ready {
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
    }

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
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{
        Request,
        StatusCode,
    };
    use axum::routing::get;
    use axum::Router;
    use pipedash_core::infrastructure::{
        PipedashConfig,
        StorageBackendType,
        StorageManager,
    };
    use pipedash_core::CoreContext;
    use tower::ServiceExt;

    use super::*;
    use crate::ws::WebSocketEventBus;

    async fn get_health(app: &Router) -> (StatusCode, serde_json::Value) {
        let response = app
            .clone()
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();

        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn health_reports_a_failed_database_query_as_unavailable() {
        let _env = crate::routes::CONFIG_ENV.lock().await;
        let data_dir = std::env::temp_dir().join(format!("pipedash-health-{}", std::process::id()));
        std::env::set_var("PIPEDASH_CONFIG_PATH", data_dir.join("config.toml"));
        let mut config = PipedashConfig::default();
        config.storage.backend = StorageBackendType::Sqlite;
        config.storage.data_dir = data_dir.display().to_string();
        config.storage.vault_password = Some("vault-password".into());
        let storage_manager = StorageManager::from_config(config, false).await.unwrap();
        let ws_event_bus = Arc::new(WebSocketEventBus::new());
        let core = CoreContext::with_storage_manager(&storage_manager, ws_event_bus.clone())
            .await
            .unwrap();
        let pool = core
            .provider_service
            .repository()
            .cache_pool()
            .as_sqlite()
            .unwrap()
            .clone();
        let state = AppState::initialized(ws_event_bus, core, storage_manager);
        state.inner.write().await.token_store_ready = true;
        let app = Router::new()
            .route("/health", get(health_check))
            .with_state(state);

        let (status, body) = get_health(&app).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "ok");
        assert_eq!(body["database"]["connected"], true);

        pool.close().await;

        let (failed_status, failed_body) = get_health(&app).await;
        let _ = std::fs::remove_dir_all(&data_dir);
        assert_eq!(failed_status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(failed_body["status"], "database_error");
        assert_eq!(failed_body["database"]["connected"], false);
    }
}
