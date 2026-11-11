use std::io::{
    Read,
    Write,
};
use std::net::TcpListener;
use std::sync::Arc;

use reqwest::header::HeaderValue;

use super::*;

fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.insert(*name, HeaderValue::from_str(value).unwrap());
    }
    map
}

fn fixed_now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-22T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

#[test]
fn require_client_returns_same_arc_or_errors() {
    install_crypto_provider();
    let client = Arc::new(reqwest::Client::new());
    let got = require_client(Some(client.clone())).unwrap();
    assert!(Arc::ptr_eq(&client, &got));
    assert!(matches!(
        require_client(None),
        Err(PluginError::Internal(message)) if message == "HTTP client was not provided"
    ));
}

#[test]
fn retry_after_parses_seconds_and_dates() {
    let now = fixed_now();
    assert_eq!(
        retry_after(&headers(&[("retry-after", "120")]), now),
        Some(Duration::from_secs(120))
    );
    assert_eq!(
        retry_after(
            &headers(&[("retry-after", "Thu, 22 Oct 2026 12:01:30 GMT")]),
            now
        ),
        Some(Duration::from_secs(90))
    );
    assert_eq!(
        retry_after(
            &headers(&[("retry-after", "Thu, 22 Oct 2026 11:00:00 GMT")]),
            now
        ),
        Some(Duration::ZERO)
    );
    assert_eq!(retry_after(&headers(&[("retry-after", "soon")]), now), None);
    assert_eq!(retry_after(&HeaderMap::new(), now), None);
}

#[test]
fn retry_after_reads_reset_as_epoch_or_delta() {
    let now = fixed_now();
    let epoch = (now.timestamp() + 30).to_string();
    assert_eq!(
        retry_after(&headers(&[("x-ratelimit-reset", &epoch)]), now),
        Some(Duration::from_secs(30))
    );
    assert_eq!(
        retry_after(&headers(&[("ratelimit-reset", "45")]), now),
        Some(Duration::from_secs(45))
    );
    assert_eq!(
        retry_after(&headers(&[("x-ratelimit-reset", "1000000000")]), now),
        Some(Duration::ZERO)
    );
}

#[test]
fn retry_after_is_capped_and_saturating() {
    let now = fixed_now();
    assert_eq!(
        retry_after(&headers(&[("retry-after", "86400")]), now),
        Some(MAX_RETRY_AFTER)
    );
    assert_eq!(
        retry_after(&headers(&[("retry-after", &u64::MAX.to_string())]), now),
        Some(MAX_RETRY_AFTER)
    );
    assert_eq!(
        retry_after(
            &headers(&[("x-ratelimit-reset", &u64::MAX.to_string())]),
            now
        ),
        Some(MAX_RETRY_AFTER)
    );
}

#[test]
fn retry_after_prefers_retry_after_header() {
    let now = fixed_now();
    assert_eq!(
        retry_after(
            &headers(&[("retry-after", "5"), ("x-ratelimit-reset", "500")]),
            now
        ),
        Some(Duration::from_secs(5))
    );
}

#[test]
fn truncate_chars_cuts_on_char_boundaries() {
    assert_eq!(truncate_chars("héllo wörld", 4), "héll…");
    assert_eq!(truncate_chars("日本語テキスト", 3), "日本語…");
    assert_eq!(truncate_chars("short", 10), "short");
    assert!(matches!(truncate_chars("short", 5), Cow::Borrowed(_)));
}

#[test]
fn redact_url_drops_userinfo_query_and_fragment() {
    let url = Url::parse("https://user:secret@example.com/api/v1?token=abc#frag").unwrap();
    assert_eq!(redact_url(&url), "https://example.com/api/v1");
}

fn serve_once(response: impl Into<String>) -> Url {
    let response = response.into();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(response.as_bytes());
        }
    });
    Url::parse(&format!("http://{addr}/path?token=secret")).unwrap()
}

fn serve_held_open(head: &'static str, sent: usize) -> Url {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&vec![b'x'; sent]);
            let _ = stream.read(&mut buf);
        }
    });
    Url::parse(&format!("http://{addr}/")).unwrap()
}

#[tokio::test]
async fn reads_stop_at_the_limit_without_waiting_for_a_huge_body() {
    let huge = "HTTP/1.1 200 OK\r\nContent-Length: 1073741824\r\n\r\n";
    let resp = client()
        .get(serve_held_open(huge, 8_000))
        .send()
        .await
        .unwrap();
    let body = tokio::time::timeout(Duration::from_secs(5), read_prefix(resp, 5_000))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(body.len(), 5_000);

    let failed = "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 1073741824\r\n\r\n";
    let resp = client()
        .get(serve_held_open(failed, 8_000))
        .send()
        .await
        .unwrap();
    let err = tokio::time::timeout(Duration::from_secs(5), check_response(resp))
        .await
        .unwrap()
        .unwrap_err();
    assert!(err
        .to_string()
        .contains(&format!("{}…", "x".repeat(BODY_PREVIEW_CHARS))));
}

async fn status_error(response: impl Into<String>) -> PluginError {
    let url = serve_once(response);
    let resp = client().get(url).send().await.unwrap();
    check_response(resp).await.unwrap_err()
}

#[tokio::test]
async fn check_response_maps_every_status_row() {
    let ok = serve_once("HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n");
    let resp = client().get(ok).send().await.unwrap();
    assert!(check_response(resp).await.is_ok());

    let redirect = serve_once("HTTP/1.1 304 Not Modified\r\nContent-Length: 0\r\n\r\n");
    let resp = client().get(redirect).send().await.unwrap();
    assert!(check_response(resp).await.is_ok());

    assert!(matches!(
        status_error("HTTP/1.1 401 Unauthorized\r\nContent-Length: 4\r\n\r\nnope").await,
        PluginError::AuthenticationFailed(_)
    ));
    assert!(matches!(
        status_error(
            "HTTP/1.1 403 Forbidden\r\nx-ratelimit-remaining: 0\r\nContent-Length: 0\r\n\r\n"
        )
        .await,
        PluginError::RateLimited { .. }
    ));
    assert!(matches!(
        status_error("HTTP/1.1 403 Forbidden\r\nretry-after: 30\r\nContent-Length: 0\r\n\r\n")
            .await,
        PluginError::RateLimited {
            retry_after: Some(d),
            ..
        } if d == Duration::from_secs(30)
    ));
    assert!(matches!(
        status_error("HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n").await,
        PluginError::PermissionDenied(_)
    ));
    assert!(matches!(
        status_error("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n").await,
        PluginError::PipelineNotFound(_)
    ));
    assert!(matches!(
        status_error(
            "HTTP/1.1 429 Too Many Requests\r\nretry-after: 7\r\nContent-Length: 0\r\n\r\n"
        )
        .await,
        PluginError::RateLimited {
            retry_after: Some(d),
            ..
        } if d == Duration::from_secs(7)
    ));
    assert!(matches!(
        status_error("HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n").await,
        PluginError::ServiceUnavailable {
            retry_after: None,
            ..
        }
    ));
    assert!(matches!(
        status_error("HTTP/1.1 418 I'm a teapot\r\nContent-Length: 0\r\n\r\n").await,
        PluginError::ApiError(_)
    ));
}

#[tokio::test]
async fn check_response_message_is_redacted_and_truncated() {
    let content = "é".repeat(400);
    let body = format!(
        "HTTP/1.1 500 Internal Server Error\r\nContent-Length: {}\r\n\r\n{content}",
        content.len()
    );
    let err = status_error(body).await;
    let text = err.to_string();
    assert!(!text.contains("token=secret"));
    assert!(text.contains("500 from http://127.0.0.1"));
    assert!(text.ends_with('…'));
}

fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn client() -> reqwest::Client {
    install_crypto_provider();
    reqwest::Client::new()
}

fn spawn_tls_server() -> Url {
    install_crypto_provider();
    let cert = rustls::pki_types::CertificateDer::from(
        include_bytes!("../../tests/fixtures/self_signed_cert.der").to_vec(),
    );
    let key = rustls::pki_types::PrivateKeyDer::Pkcs8(
        include_bytes!("../../tests/fixtures/self_signed_key.der")
            .to_vec()
            .into(),
    );
    let config = Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            if let Ok(mut conn) = rustls::ServerConnection::new(config) {
                let _ = conn.complete_io(&mut stream);
            }
        }
    });
    Url::parse(&format!(
        "https://localhost:{}/api?token=secret",
        addr.port()
    ))
    .unwrap()
}

#[tokio::test]
async fn map_send_error_detects_untrusted_certificate() {
    let url = spawn_tls_server();
    let err = client().get(url.clone()).send().await.unwrap_err();
    let mapped = map_send_error(err, &url);
    assert!(matches!(mapped, PluginError::Tls(_)), "{mapped:?}");
    assert!(!mapped.to_string().contains("token=secret"));
}

#[tokio::test]
async fn map_send_error_maps_connect_failure_to_network() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let url = Url::parse(&format!("http://{addr}/?token=secret")).unwrap();
    let err = client().get(url.clone()).send().await.unwrap_err();
    let mapped = map_send_error(err, &url);
    assert!(matches!(mapped, PluginError::NetworkError(_)), "{mapped:?}");
    assert!(!mapped.to_string().contains("token=secret"));
}

#[test]
fn with_query_appends_only_when_given_pairs() {
    let url = Url::parse("https://example.com/api").unwrap();
    assert_eq!(with_query(url.clone(), &[]), url);
    let url = with_query(url, &[("page", "2".to_string()), ("q", "a b".to_string())]);
    assert_eq!(url.query(), Some("page=2&q=a+b"));
}

#[tokio::test]
async fn get_json_decodes_success_and_maps_errors() {
    let retry = RetryPolicy::new(1, Duration::from_millis(1), false);
    let ok = serve_once("HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\n{\"a\":42}");
    let value: serde_json::Value = get_json(&retry, &ok, "thing", || client().get(ok.clone()))
        .await
        .unwrap();
    assert_eq!(value["a"], 42);

    let missing = serve_once("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
    let error =
        get_json::<serde_json::Value>(&retry, &missing, "thing", || client().get(missing.clone()))
            .await
            .unwrap_err();
    assert!(
        matches!(error, PluginError::PipelineNotFound(_)),
        "{error:?}"
    );
}

#[tokio::test]
async fn send_checked_rejects_error_statuses() {
    let retry = RetryPolicy::new(1, Duration::from_millis(1), false);
    let url = serve_once("HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n");
    let error = send_checked(&retry, &url, || client().post(url.clone()))
        .await
        .unwrap_err();
    assert!(
        matches!(error, PluginError::AuthenticationFailed(_)),
        "{error:?}"
    );
}

#[test]
fn api_url_joins_base_path_and_query() {
    let url = api_url(
        "https://ci.dev/api",
        "/repos",
        &[("page", "2".to_string())],
        "CI",
    )
    .unwrap();
    assert_eq!(url.as_str(), "https://ci.dev/api/repos?page=2");
    let error = api_url("ci.dev", "/x", &[], "CI").unwrap_err();
    assert!(matches!(error, PluginError::InvalidConfig(_)));
}
