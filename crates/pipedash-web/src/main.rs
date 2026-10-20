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
        header::{
            AUTHORIZATION,
            HOST,
            ORIGIN,
        },
        Method,
        StatusCode,
    },
    middleware::Next,
    response::Response,
    Router,
};
use pipedash_core::infrastructure::{
    ConfigLoader,
    Platform,
    ServerConfig,
    StorageBackendType,
    StorageManager,
};
use pipedash_core::CoreContext;
use subtle::ConstantTimeEq;
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

fn token_matches(provided: &str, expected: &str) -> bool {
    provided.as_bytes().ct_eq(expected.as_bytes()).into()
}

fn bearer_matches(auth_header: Option<&str>, token: &str) -> bool {
    auth_header
        .and_then(|h| h.strip_prefix("Bearer "))
        .is_some_and(|t| token_matches(t, token))
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

    if bearer_matches(auth_header, &token) {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn is_cross_origin_write(req: &Request) -> bool {
    if matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        return false;
    }

    let headers = req.headers();
    if let Some(site) = headers.get("sec-fetch-site") {
        return !matches!(site.as_bytes(), b"same-origin" | b"none");
    }

    let Some(origin) = headers.get(ORIGIN) else {
        return false;
    };
    let origin_host = origin
        .to_str()
        .ok()
        .and_then(|o| o.split_once("://"))
        .map(|(_, host)| host);
    let host = headers
        .get(HOST)
        .and_then(|h| h.to_str().ok())
        .or_else(|| req.uri().authority().map(|a| a.as_str()));

    !matches!((origin_host, host), (Some(o), Some(h)) if o.eq_ignore_ascii_case(h))
}

async fn reject_cross_origin_writes(req: Request, next: Next) -> Result<Response, StatusCode> {
    if is_cross_origin_write(&req) {
        tracing::warn!(
            method = %req.method(),
            path = req.uri().path(),
            origin = ?req.headers().get(ORIGIN),
            "Rejected cross-origin request. Set PIPEDASH_CORS_ALLOW_ALL=true to allow other origins"
        );
        return Err(StatusCode::FORBIDDEN);
    }

    Ok(next.run(req).await)
}

fn env_flag(value: &str) -> bool {
    value == "true" || value == "1"
}

impl ApiServerConfig {
    fn resolve(file: Option<&ServerConfig>) -> anyhow::Result<Self> {
        Self::from_sources(
            std::env::var("PIPEDASH_BIND_ADDR").ok().as_deref(),
            std::env::var("PIPEDASH_CORS_ALLOW_ALL").ok().as_deref(),
            std::env::var("PIPEDASH_EMBEDDED_FRONTEND").ok().as_deref(),
            file,
        )
    }

    fn from_sources(
        env_bind_addr: Option<&str>, env_cors_allow_all: Option<&str>,
        env_embedded_frontend: Option<&str>, file: Option<&ServerConfig>,
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
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    pipedash_core::logging::init();

    let mut file_server_config = None;

    tracing::info!("Starting Pipedash API server");

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

    let api_config = ApiServerConfig::resolve(file_server_config.as_ref())?;

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

    let mut app = Router::new()
        .nest("/api/v1", routes::api_router())
        .route("/api/v1/ws", axum::routing::get(ws::ws_handler))
        .fallback(if api_config.enable_embedded_frontend {
            axum::routing::get(static_files::serve_static)
        } else {
            axum::routing::get(|| async { axum::http::StatusCode::NOT_FOUND })
        })
        .layer(axum::middleware::from_fn(auth_middleware));

    if !api_config.cors_allow_all {
        app = app.layer(axum::middleware::from_fn(reject_cross_origin_writes));
    }

    let app = app
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer(api_config.cors_allow_all))
        .with_state(app_state);

    tracing::info!("Listening on {}", api_config.bind_addr);
    let listener = tokio::net::TcpListener::bind(api_config.bind_addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::header;
    use axum::routing::get;
    use tower::ServiceExt;

    use super::*;

    #[test]
    fn bearer_matches_accepts_exact_token() {
        assert!(bearer_matches(
            Some("Bearer operator-secret"),
            "operator-secret"
        ));
    }

    #[test]
    fn bearer_matches_rejects_missing_wrong_and_partial_tokens() {
        for auth_header in [
            None,
            Some("operator-secret"),
            Some("Bearer "),
            Some("Bearer wrong"),
            Some("Bearer operator"),
            Some("Bearer operator-secret-extra"),
        ] {
            assert!(
                !bearer_matches(auth_header, "operator-secret"),
                "accepted {auth_header:?}"
            );
        }
    }

    fn server_config(bind_addr: &str, cors_allow_all: bool) -> ServerConfig {
        ServerConfig {
            bind_addr: bind_addr.to_string(),
            cors_allow_all,
        }
    }

    #[test]
    fn setup_mode_falls_back_to_secure_defaults() {
        let config = ApiServerConfig::from_sources(None, None, None, None).unwrap();

        assert_eq!(config.bind_addr, "127.0.0.1:8080".parse().unwrap());
        assert!(!config.cors_allow_all);
        assert!(config.enable_embedded_frontend);
    }

    #[test]
    fn config_values_apply_when_env_is_unset() {
        let file = server_config("0.0.0.0:9090", true);

        let config = ApiServerConfig::from_sources(None, None, None, Some(&file)).unwrap();

        assert_eq!(config.bind_addr, "0.0.0.0:9090".parse().unwrap());
        assert!(config.cors_allow_all);
    }

    #[test]
    fn env_overrides_config_values() {
        let file = server_config("0.0.0.0:9090", true);

        let config = ApiServerConfig::from_sources(
            Some("127.0.0.1:7070"),
            Some("false"),
            Some("false"),
            Some(&file),
        )
        .unwrap();

        assert_eq!(config.bind_addr, "127.0.0.1:7070".parse().unwrap());
        assert!(!config.cors_allow_all);
        assert!(!config.enable_embedded_frontend);

        let file = server_config("0.0.0.0:9090", false);
        let config = ApiServerConfig::from_sources(None, Some("1"), None, Some(&file)).unwrap();

        assert!(config.cors_allow_all);
    }

    #[test]
    fn invalid_bind_addr_is_an_error() {
        let file = server_config("not-an-address", false);

        assert!(ApiServerConfig::from_sources(None, None, None, Some(&file)).is_err());
        assert!(ApiServerConfig::from_sources(Some("nope"), None, None, None).is_err());
    }

    #[tokio::test]
    async fn default_cors_does_not_allow_other_origins() {
        let config = ApiServerConfig::from_sources(None, None, None, None).unwrap();
        let app = Router::new()
            .route("/api/v1/providers", get(|| async { "ok" }))
            .layer(cors_layer(config.cors_allow_all));
        let request = axum::http::Request::builder()
            .uri("/api/v1/providers")
            .header(header::ORIGIN, "https://attacker.example")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert!(!response
            .headers()
            .contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN));
    }

    async fn factory_reset_status(method: Method, headers: &[(&str, &str)]) -> StatusCode {
        let app = Router::new()
            .route(
                "/api/v1/factory-reset",
                axum::routing::any(|| async { "ok" }),
            )
            .layer(axum::middleware::from_fn(reject_cross_origin_writes));
        let mut request = axum::http::Request::builder()
            .method(method)
            .uri("/api/v1/factory-reset")
            .header(header::HOST, "127.0.0.1:8080");
        for (name, value) in headers {
            request = request.header(*name, *value);
        }

        app.oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn cross_origin_writes_are_forbidden() {
        for headers in [
            &[("origin", "http://127.0.0.1:8899")][..],
            &[("origin", "null")],
            &[("origin", "http://127.0.0.1:8080.attacker.example")],
            &[
                ("origin", "http://127.0.0.1:8899"),
                ("sec-fetch-site", "same-site"),
            ],
            &[
                ("origin", "http://127.0.0.1:8080"),
                ("sec-fetch-site", "cross-site"),
            ],
        ] {
            assert_eq!(
                factory_reset_status(Method::POST, headers).await,
                StatusCode::FORBIDDEN,
                "allowed {headers:?}"
            );
        }
    }

    #[tokio::test]
    async fn same_origin_and_non_browser_writes_are_allowed() {
        for headers in [
            &[][..],
            &[("origin", "http://127.0.0.1:8080")],
            &[
                ("origin", "http://localhost:1420"),
                ("sec-fetch-site", "same-origin"),
            ],
        ] {
            assert_eq!(
                factory_reset_status(Method::POST, headers).await,
                StatusCode::OK,
                "rejected {headers:?}"
            );
        }
    }

    #[tokio::test]
    async fn cross_origin_reads_are_allowed() {
        let headers = [
            ("origin", "http://127.0.0.1:8899"),
            ("sec-fetch-site", "same-site"),
        ];

        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert_eq!(factory_reset_status(method, &headers).await, StatusCode::OK);
        }
    }
}
