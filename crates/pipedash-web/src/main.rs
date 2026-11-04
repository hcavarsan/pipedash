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
use pipedash_web::ws::WebSocketEventBus;
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
                drop(core.start_background_tasks().await);
            }
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
mod tests {
    use axum::body::Body;
    use axum::http::header;
    use axum::http::{
        Method,
        StatusCode,
    };
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    use super::*;

    fn server_config(bind_addr: &str, cors_allow_all: bool) -> ServerConfig {
        ServerConfig {
            bind_addr: bind_addr.to_string(),
            cors_allow_all,
            base_path: String::new(),
        }
    }

    #[test]
    fn setup_mode_falls_back_to_secure_defaults() {
        let config = ApiServerConfig::from_sources(None, None, None, None, None).unwrap();

        assert_eq!(config.bind_addr, "127.0.0.1:8080".parse().unwrap());
        assert!(!config.cors_allow_all);
        assert!(config.enable_embedded_frontend);
    }

    #[test]
    fn config_values_apply_when_env_is_unset() {
        let file = server_config("0.0.0.0:9090", true);

        let config = ApiServerConfig::from_sources(None, None, None, None, Some(&file)).unwrap();

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
            None,
            Some(&file),
        )
        .unwrap();

        assert_eq!(config.bind_addr, "127.0.0.1:7070".parse().unwrap());
        assert!(!config.cors_allow_all);
        assert!(!config.enable_embedded_frontend);

        let file = server_config("0.0.0.0:9090", false);
        let config =
            ApiServerConfig::from_sources(None, Some("1"), None, None, Some(&file)).unwrap();

        assert!(config.cors_allow_all);
    }

    #[test]
    fn invalid_bind_addr_is_an_error() {
        let file = server_config("not-an-address", false);

        assert!(ApiServerConfig::from_sources(None, None, None, None, Some(&file)).is_err());
        assert!(ApiServerConfig::from_sources(Some("nope"), None, None, None, None).is_err());
    }

    #[test]
    fn base_path_comes_from_env_or_file_with_slashes_added() {
        let mut file = server_config("127.0.0.1:8080", false);
        let resolve = |env: Option<&str>, file: &ServerConfig| {
            ApiServerConfig::from_sources(None, None, None, env, Some(file)).map(|c| c.base_path)
        };

        assert_eq!(resolve(None, &file).unwrap(), "/");
        file.base_path = "pipedash".into();
        assert_eq!(resolve(None, &file).unwrap(), "/pipedash/");
        assert_eq!(resolve(Some("/tools/ci/"), &file).unwrap(), "/tools/ci/");
        assert_eq!(resolve(Some("/"), &file).unwrap(), "/");
        for invalid in ["/a\"><script>", "/a b", "/../x", "/a//b"] {
            assert!(
                resolve(Some(invalid), &file).is_err(),
                "accepted {invalid:?}"
            );
        }
    }

    #[tokio::test]
    async fn default_cors_does_not_allow_other_origins() {
        let config = ApiServerConfig::from_sources(None, None, None, None, None).unwrap();
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
            .layer(axum::middleware::from_fn(reject_cross_origin_requests));
        let mut request = axum::http::Request::builder()
            .method(method)
            .uri("/api/v1/factory-reset");
        if !headers.iter().any(|(name, _)| *name == "host") {
            request = request.header(header::HOST, "127.0.0.1:8080");
        }
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
            &[("origin", "https://127.0.0.1:8080")],
            &[("origin", "https://example.com"), ("host", "example.com")],
            &[
                ("origin", "http://example.com"),
                ("host", "example.com"),
                ("x-forwarded-proto", "https"),
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
            &[
                ("origin", "https://pipedash.example.com"),
                ("host", "pipedash.example.com"),
                ("x-forwarded-proto", "https"),
            ],
            &[
                ("origin", "https://pipedash.example.com"),
                ("host", "pipedash.example.com:443"),
                ("x-forwarded-proto", "https, http"),
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

    async fn websocket_handshake(origin_layer: bool, extra: &[(&str, &str)]) -> String {
        use tokio::io::{
            AsyncReadExt,
            AsyncWriteExt,
        };

        let state = AppState::setup_mode(Arc::new(WebSocketEventBus::new()));
        assert!(state.api_token().is_none());
        let mut router = app(state, None);
        if origin_layer {
            router = router.layer(axum::middleware::from_fn(reject_cross_origin_requests));
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let mut request = format!(
            "GET /api/v1/ws HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             Sec-WebSocket-Version: 13\r\n"
        );
        for (name, value) in extra {
            request.push_str(&format!(
                "{name}: {}\r\n",
                value.replace("{addr}", &addr.to_string())
            ));
        }
        request.push_str("\r\n");
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        while !response.ends_with(b"\r\n\r\n") {
            response.push(stream.read_u8().await.unwrap());
        }

        String::from_utf8_lossy(&response)
            .lines()
            .next()
            .unwrap_or_default()
            .to_string()
    }

    #[tokio::test]
    async fn a_websocket_upgrade_from_a_foreign_origin_is_rejected_without_a_token() {
        for extra in [
            &[("Origin", "http://evil.example")][..],
            &[
                ("Origin", "http://evil.example"),
                ("Sec-Fetch-Site", "cross-site"),
            ],
            &[
                ("Origin", "http://localhost:1420"),
                ("Sec-Fetch-Site", "same-site"),
            ],
            &[("Origin", "http://{addr}.evil.example")],
        ] {
            let status = websocket_handshake(true, extra).await;

            assert!(status.contains(" 403"), "{extra:?} -> {status}");
        }
    }

    #[tokio::test]
    async fn a_websocket_upgrade_from_the_same_origin_or_a_non_browser_is_accepted() {
        for extra in [
            &[][..],
            &[("Origin", "http://{addr}")],
            &[
                ("Origin", "http://localhost:1420"),
                ("Sec-Fetch-Site", "same-origin"),
            ],
        ] {
            let status = websocket_handshake(true, extra).await;

            assert!(status.contains(" 101"), "{extra:?} -> {status}");
        }
    }

    #[tokio::test]
    async fn with_cors_allowed_for_all_any_origin_may_upgrade() {
        let status = websocket_handshake(false, &[("Origin", "http://evil.example")]).await;

        assert!(status.contains(" 101"), "{status}");
    }

    #[test]
    fn the_trace_span_never_carries_a_download_ticket() {
        let ticket: Uri = "/api/v1/downloads/0123456789abcdef".parse().unwrap();
        let other: Uri = "/api/v1/providers?x=1".parse().unwrap();

        assert_eq!(redact_ticket(&ticket), "/api/v1/downloads/[redacted]");
        assert_eq!(redact_ticket(&other), "/api/v1/providers?x=1");
    }
}
