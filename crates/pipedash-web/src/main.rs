mod error;
mod routes;
mod state;
mod static_files;
mod ws;

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use axum::{
    extract::Request,
    http::{
        header::AUTHORIZATION,
        StatusCode,
    },
    middleware::Next,
    response::Response,
    Router,
};
use pipedash_core::infrastructure::{
    ConfigLoader,
    Platform,
    StorageBackendType,
    StorageManager,
};
use pipedash_core::CoreContext;
use tower_http::cors::{
    Any,
    CorsLayer,
};
use tower_http::trace::TraceLayer;

use crate::state::AppState;
use crate::ws::WebSocketEventBus;

struct ApiServerConfig {
    bind_addr: SocketAddr,
    cors_allow_all: bool,
    enable_embedded_frontend: bool,
}

/// Get the current API auth token from environment variable.
/// This is read dynamically to support vault unlock/lock operations.
fn get_api_auth_token() -> Option<String> {
    std::env::var("PIPEDASH_VAULT_PASSWORD").ok()
}

async fn auth_middleware(req: Request, next: Next) -> Result<Response, StatusCode> {
    let path = req.uri().path();

    if !path.starts_with("/api/v1/")
        || path.starts_with("/api/v1/health")
        || path.starts_with("/api/v1/setup")
        || path.starts_with("/api/v1/vault")
        || path == "/api/v1/ws"
        || path == "/api/v1/plugins"
    {
        return Ok(next.run(req).await);
    }

    let token = match get_api_auth_token() {
        Some(t) => t,
        None => return Ok(next.run(req).await),
    };

    let auth_header = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(h)
            if h.strip_prefix("Bearer ")
                .map(|t| t == token)
                .unwrap_or(false) =>
        {
            Ok(next.run(req).await)
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

impl ApiServerConfig {
    fn from_env() -> Self {
        let bind_addr = std::env::var("PIPEDASH_BIND_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
            .parse()
            .expect("Invalid bind address");

        let cors_allow_all = std::env::var("PIPEDASH_CORS_ALLOW_ALL")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(true);

        let enable_embedded_frontend = std::env::var("PIPEDASH_EMBEDDED_FRONTEND")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(true); // Default to true for production

        Self {
            bind_addr,
            cors_allow_all,
            enable_embedded_frontend,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    pipedash_core::logging::init();

    let api_config = ApiServerConfig::from_env();

    tracing::info!("Starting Pipedash API server");
    tracing::info!("Bind address: {}", api_config.bind_addr);

    if api_config.enable_embedded_frontend {
        tracing::info!(
            "Serving embedded frontend at http://{}",
            api_config.bind_addr
        );
    } else {
        tracing::info!(
            "Embedded frontend disabled - expecting external frontend (e.g., Vite dev server)"
        );
    }

    let ws_event_bus = Arc::new(WebSocketEventBus::new());

    let config_path = ConfigLoader::discover_config_path();
    let data_dir = config_path.parent().unwrap_or(std::path::Path::new("."));
    let setup_status = ConfigLoader::get_setup_status(data_dir);

    tracing::info!(
        "Setup status: config_exists={}, config_valid={}, needs_setup={}, needs_migration={}",
        setup_status.config_exists,
        setup_status.config_valid,
        setup_status.needs_setup,
        setup_status.needs_migration
    );

    let app_state = if setup_status.needs_setup {
        tracing::info!("Setup required - starting in setup mode");
        AppState::setup_mode(ws_event_bus.clone())
    } else if !setup_status.config_valid {
        let error_msg = setup_status.validation_errors.join(", ");
        tracing::error!("Invalid configuration: {}", error_msg);
        AppState::config_error(ws_event_bus.clone(), error_msg)
    } else {
        tracing::info!("Configuration valid - initializing services");

        let config = ConfigLoader::load_or_create(&config_path, Platform::Server)
            .context("Failed to load config")?;

        tracing::info!(
            "Initializing with storage backend: {}",
            config.storage.backend,
        );

        #[cfg(feature = "postgres")]
        if config.storage.backend == StorageBackendType::Postgres {
            use pipedash_core::infrastructure::database::init_postgres_database;

            tracing::info!("Running PostgreSQL migrations on startup...");

            init_postgres_database(&config.storage.postgres.connection_string)
                .await
                .context("Failed to initialize PostgreSQL database")?;

            tracing::info!("PostgreSQL migrations completed");
        }

        let storage_manager = StorageManager::from_config_allow_locked(config.clone(), false)
            .await
            .context("Failed to initialize storage manager")?;

        let vault_locked = storage_manager.is_vault_locked().await;
        if vault_locked {
            tracing::warn!("Vault is locked - server starting in locked mode");
            tracing::warn!(
                "Set PIPEDASH_VAULT_PASSWORD environment variable or unlock via /api/v1/vault/unlock"
            );
        }

        let core_context =
            CoreContext::with_storage_manager(&storage_manager, ws_event_bus.clone()).await?;

        let app_state = AppState::initialized(ws_event_bus.clone(), core_context, storage_manager);

        if !vault_locked {
            tracing::info!("Warming up token store before starting background tasks...");
            {
                let inner = app_state.inner.read().await;
                let core = inner.core.as_ref().unwrap();
                core.warmup_token_store()
                    .await
                    .context("Failed to warm up token store")?;
            }

            {
                let mut inner = app_state.inner.write().await;
                inner.token_store_ready = true;
            }
            tracing::info!("Token store warmup complete - system ready");

            {
                let inner = app_state.inner.read().await;
                let core = inner.core.as_ref().unwrap();
                core.start_background_tasks().await;
            }
        } else {
            tracing::info!(
                "Vault locked - skipping token store warmup. Unlock vault to enable providers."
            );
        }

        app_state
    };

    let app = Router::new()
        .nest("/api/v1", routes::api_router())
        .route("/api/v1/ws", axum::routing::get(ws::ws_handler))
        .fallback(if api_config.enable_embedded_frontend {
            axum::routing::get(static_files::serve_static)
        } else {
            axum::routing::get(|| async { axum::http::StatusCode::NOT_FOUND })
        })
        .layer(axum::middleware::from_fn(auth_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(if api_config.cors_allow_all {
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any)
        } else {
            CorsLayer::new()
        })
        .with_state(app_state);

    tracing::info!("Listening on {}", api_config.bind_addr);
    let listener = tokio::net::TcpListener::bind(api_config.bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
