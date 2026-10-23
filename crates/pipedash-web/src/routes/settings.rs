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
    state
        .config_state()
        .await?
        .update_general_settings(update)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{
        header,
        Method,
        Request,
    };
    use pipedash_core::infrastructure::{
        ConfigLoader,
        PipedashConfig,
        StorageBackendType,
        StorageManager,
    };
    use pipedash_core::CoreContext;
    use tower::ServiceExt;

    use super::*;
    use crate::ws::WebSocketEventBus;

    async fn put_general(app: &Router, body: &str) -> StatusCode {
        app.clone()
            .oneshot(
                Request::builder()
                    .method(Method::PUT)
                    .uri("/settings/general")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn general_settings_are_validated_saved_and_read_back_with_the_new_field() {
        let _env = crate::routes::CONFIG_ENV.lock().await;
        let data_dir =
            std::env::temp_dir().join(format!("pipedash-settings-{}", std::process::id()));
        let config_path = data_dir.join("config.toml");
        std::env::set_var("PIPEDASH_CONFIG_PATH", &config_path);
        let mut config = PipedashConfig::default();
        config.storage.backend = StorageBackendType::Sqlite;
        config.storage.data_dir = data_dir.display().to_string();
        config.storage.vault_password = Some("vault-password".into());
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::write(&config_path, "[storage]\nbackend = \"sqlite\"\n").unwrap();
        let storage_manager = StorageManager::from_config(config, false).await.unwrap();
        let ws_event_bus = Arc::new(WebSocketEventBus::new());
        let core = CoreContext::with_storage_manager(&storage_manager, ws_event_bus.clone())
            .await
            .unwrap();
        let state = AppState::initialized(ws_event_bus, core, storage_manager);
        let app = Router::new().nest("/settings", router()).with_state(state);

        let too_fast = put_general(
            &app,
            r#"{"metrics_enabled":true,"default_refresh_interval":4,"status_pages_enabled":true}"#,
        )
        .await;
        let too_slow = put_general(
            &app,
            r#"{"metrics_enabled":true,"default_refresh_interval":301,"status_pages_enabled":true}"#,
        )
        .await;
        let unchanged = ConfigLoader::load(&config_path).unwrap();
        let saved = put_general(
            &app,
            r#"{"metrics_enabled":false,"default_refresh_interval":45,"status_pages_enabled":false}"#,
        )
        .await;
        let reloaded = ConfigLoader::load(&config_path).unwrap();
        let _ = std::fs::remove_dir_all(&data_dir);

        assert_eq!(too_fast, StatusCode::BAD_REQUEST);
        assert_eq!(too_slow, StatusCode::BAD_REQUEST);
        assert!(unchanged.general.status_pages_enabled);
        assert_eq!(saved, StatusCode::NO_CONTENT);
        assert!(!reloaded.general.status_pages_enabled);
        assert!(!reloaded.general.metrics_enabled);
        assert_eq!(reloaded.general.default_refresh_interval, 45);
    }
}
