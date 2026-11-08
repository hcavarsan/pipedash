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
    let config = ApiServerConfig::from_sources(None, Some("1"), None, None, Some(&file)).unwrap();

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

    let state = AppState::setup_mode(
        Arc::new(WebSocketEventBus::new()),
        Arc::new(pipedash_plugins::registry()),
    );
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
