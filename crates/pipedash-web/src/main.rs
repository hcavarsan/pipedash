use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use axum::{
    extract::Request,
    http::Uri,
};
use pipedash_core::infrastructure::{
    ConfigLoader,
    Platform,
    ServerConfig,
    StorageBackendType,
    StorageManager,
};
use pipedash_core::CoreContext;
use pipedash_web::state::AppState;
use pipedash_web::ws::event_bus::WebSocketEventBus;
use pipedash_web::{
    app,
    demo,
    env_flag,
    push,
    reject_cross_origin_requests,
};
use tower_http::cors::{
    Any,
    CorsLayer,
};
use tower_http::trace::TraceLayer;

struct ApiServerConfig {
    bind_addr: SocketAddr,
    cors_allow_all: bool,
    enable_embedded_frontend: bool,
    base_path: String,
}

fn cors_layer(allow_all: bool) -> CorsLayer {
    if allow_all {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        CorsLayer::new()
    }
}

fn redact_ticket(uri: &Uri) -> Cow<'_, str> {
    if uri.path().contains("/api/v1/downloads/") {
        Cow::Borrowed("/api/v1/downloads/[redacted]")
    } else {
        Cow::Owned(uri.to_string())
    }
}

fn normalize_base_path(raw: &str) -> anyhow::Result<String> {
    let trimmed = raw.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Ok("/".to_string());
    }

    let valid_chars = trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-._~/".contains(c));
    let valid_segments = trimmed
        .split('/')
        .all(|segment| !segment.is_empty() && segment != "." && segment != "..");
    if !valid_chars || !valid_segments {
        anyhow::bail!(
            "Invalid base path {raw:?}: use path segments made of letters, digits, '-', '.', '_' and '~'"
        );
    }

    Ok(format!("/{trimmed}/"))
}

impl ApiServerConfig {
    fn resolve(file: Option<&ServerConfig>) -> anyhow::Result<Self> {
        Self::from_sources(
            std::env::var("PIPEDASH_BIND_ADDR").ok().as_deref(),
            std::env::var("PIPEDASH_CORS_ALLOW_ALL").ok().as_deref(),
            std::env::var("PIPEDASH_EMBEDDED_FRONTEND").ok().as_deref(),
            std::env::var("PIPEDASH_BASE_PATH").ok().as_deref(),
            file,
        )
    }

    fn from_sources(
        env_bind_addr: Option<&str>, env_cors_allow_all: Option<&str>,
        env_embedded_frontend: Option<&str>, env_base_path: Option<&str>,
        file: Option<&ServerConfig>,
    ) -> anyhow::Result<Self> {
        let defaults = ServerConfig::default();
        let file = file.unwrap_or(&defaults);

        let bind_addr = env_bind_addr
            .unwrap_or(&file.bind_addr)
            .parse()
            .context("Invalid bind address")?;

        Ok(Self {
            bind_addr,
            cors_allow_all: env_cors_allow_all.map_or(file.cors_allow_all, env_flag),
            enable_embedded_frontend: env_embedded_frontend.is_none_or(env_flag),
            base_path: normalize_base_path(env_base_path.unwrap_or(&file.base_path))?,
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    pipedash_core::logging::init();

    let mut file_server_config = None;

    tracing::info!("Starting Pipedash API server");

    let ws_event_bus = Arc::new(WebSocketEventBus::new());
    let plugin_registry = Arc::new(pipedash_plugins::registry());

    let config_path = ConfigLoader::discover_config_path();
    let setup_status = ConfigLoader::get_setup_status(&config_path, &plugin_registry);

    tracing::info!(
        "Setup status: config_exists={}, config_valid={}, needs_setup={}, needs_migration={}",
        setup_status.config_exists,
        setup_status.config_valid,
        setup_status.needs_setup,
        setup_status.needs_migration
    );

    let app_state = if setup_status.needs_setup {
        tracing::info!("Setup required - starting in setup mode");
        AppState::setup_mode(ws_event_bus.clone(), Arc::clone(&plugin_registry))
    } else if !setup_status.config_valid {
        let error_msg = setup_status.validation_errors.join(", ");
        tracing::error!("Invalid configuration: {}", error_msg);
        AppState::config_error(
            ws_event_bus.clone(),
            error_msg,
            Arc::clone(&plugin_registry),
        )
    } else {
        tracing::info!("Configuration valid - initializing services");

        let config = ConfigLoader::load_or_create(&config_path, Platform::Server)
            .context("Failed to load config")?;

        file_server_config = Some(config.server.clone());

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

        let core_context = CoreContext::with_storage_manager(
            &storage_manager,
            ws_event_bus.clone(),
            Arc::clone(&plugin_registry),
        )
        .await?;

        let app_state = AppState::initialized(
            ws_event_bus.clone(),
            core_context.clone(),
            storage_manager,
            Arc::clone(&plugin_registry),
        );

        if !vault_locked {
            tracing::info!("Warming up token store before starting background tasks...");
            core_context
                .warmup_token_store()
                .await
                .context("Failed to warm up token store")?;

            app_state.inner.write().await.token_store_ready = true;
            tracing::info!("Token store warmup complete - system ready");

            core_context.start_background_tasks().await;
        } else {
            tracing::info!(
                "Vault locked - skipping token store warmup. Unlock vault to enable providers."
            );
        }

        app_state
    };

    if let Ok(token) = std::env::var("PIPEDASH_VAULT_PASSWORD") {
        app_state.set_api_token(&token);
    }

    push::spawn_failure_notifier(app_state.clone());
    push::spawn_alert_push(app_state.clone());

    let api_config = ApiServerConfig::resolve(file_server_config.as_ref())?;

    tracing::info!("Bind address: {}", api_config.bind_addr);

    if api_config.enable_embedded_frontend {
        tracing::info!(
            "Serving embedded frontend at http://{}{}",
            api_config.bind_addr,
            api_config.base_path
        );
    } else {
        tracing::info!(
            "Embedded frontend disabled - expecting external frontend (e.g., Vite dev server)"
        );
    }

    let mut app = app(
        app_state,
        api_config
            .enable_embedded_frontend
            .then_some(api_config.base_path),
    );

    if !api_config.cors_allow_all {
        app = app.layer(axum::middleware::from_fn(reject_cross_origin_requests));
    }

    if demo::enabled() {
        tracing::info!("Demo mode: configuration changes and pipeline actions are rejected");
    }

    let app = app
        .layer(TraceLayer::new_for_http().make_span_with(|req: &Request| {
            tracing::debug_span!(
                "request",
                method = %req.method(),
                uri = %redact_ticket(req.uri()),
                version = ?req.version()
            )
        }))
        .layer(cors_layer(api_config.cors_allow_all));

    tracing::info!("Listening on {}", api_config.bind_addr);
    let listener = tokio::net::TcpListener::bind(api_config.bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
#[path = "main/tests.rs"]
mod tests;
