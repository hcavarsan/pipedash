use std::borrow::Cow;
use std::error::Error as StdError;
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use reqwest::header::HeaderMap;
use reqwest::Url;

use crate::error::{
    PluginError,
    PluginResult,
};

const MAX_RETRY_AFTER: Duration = Duration::from_secs(3600);
const EPOCH_THRESHOLD: u64 = 1_000_000_000;
const BODY_PREVIEW_CHARS: usize = 300;

pub async fn check_response(resp: reqwest::Response) -> PluginResult<reqwest::Response> {
    let status = resp.status();
    if status.is_success() || status.is_redirection() {
        return Ok(resp);
    }

    let url = redact_url(resp.url());
    let retry_after = retry_after(resp.headers(), Utc::now());
    let rate_limit_exhausted = resp
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.trim() == "0");
    let has_retry_after = resp.headers().contains_key(reqwest::header::RETRY_AFTER);
    let body = resp.text().await.unwrap_or_default();
    let message = format!(
        "{} from {}: {}",
        status.as_u16(),
        url,
        truncate_chars(body.trim(), BODY_PREVIEW_CHARS)
    );

    if status.as_u16() == 403 && (rate_limit_exhausted || has_retry_after) {
        return Err(PluginError::RateLimited {
            message,
            retry_after,
        });
    }

    Err(from_status(status.as_u16(), message, retry_after))
}

pub fn from_status(status: u16, message: String, retry_after: Option<Duration>) -> PluginError {
    match status {
        401 => PluginError::AuthenticationFailed(message),
        403 => PluginError::PermissionDenied(message),
        404 => PluginError::PipelineNotFound(message),
        429 => PluginError::RateLimited {
            message,
            retry_after,
        },
        500..=599 => PluginError::ServiceUnavailable {
            message,
            retry_after,
        },
        _ => PluginError::ApiError(message),
    }
}

pub fn retry_after(headers: &HeaderMap, now: DateTime<Utc>) -> Option<Duration> {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
    };

    let parsed = if let Some(value) = header("retry-after") {
        parse_retry_after(value, now)
    } else {
        header("x-ratelimit-reset")
            .or_else(|| header("ratelimit-reset"))
            .and_then(|value| parse_reset(value, now))
    };

    parsed.map(|d| d.min(MAX_RETRY_AFTER))
}

fn parse_retry_after(value: &str, now: DateTime<Utc>) -> Option<Duration> {
    if let Ok(secs) = value.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    let date = DateTime::parse_from_rfc2822(value).ok()?;
    Some(until(date.with_timezone(&Utc), now))
}

fn parse_reset(value: &str, now: DateTime<Utc>) -> Option<Duration> {
    let secs = value.parse::<u64>().ok()?;
    if secs < EPOCH_THRESHOLD {
        return Some(Duration::from_secs(secs));
    }
    let now_secs = u64::try_from(now.timestamp()).unwrap_or(0);
    Some(Duration::from_secs(secs.saturating_sub(now_secs)))
}

fn until(target: DateTime<Utc>, now: DateTime<Utc>) -> Duration {
    (target - now).to_std().unwrap_or(Duration::ZERO)
}

pub fn map_send_error(err: reqwest::Error, url: &Url) -> PluginError {
    let tls = is_tls_error(&err);
    let network = err.is_timeout() || err.is_connect() || err.is_request();
    let message = format!("{} ({})", err.without_url(), redact_url(url));

    if tls {
        PluginError::Tls(message)
    } else if network {
        PluginError::NetworkError(message)
    } else {
        PluginError::ApiError(message)
    }
}

fn is_tls_error(err: &(dyn StdError + 'static)) -> bool {
    let mut current = Some(err);
    while let Some(e) = current {
        if e.downcast_ref::<rustls::Error>().is_some() {
            return true;
        }
        if let Some(inner) = e
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::get_ref)
        {
            if is_tls_error(inner) {
                return true;
            }
        }
        current = e.source();
    }
    false
}

pub fn truncate_chars(s: &str, max: usize) -> Cow<'_, str> {
    match s.char_indices().nth(max) {
        Some((idx, _)) => Cow::Owned(format!("{}…", &s[..idx])),
        None => Cow::Borrowed(s),
    }
}

pub fn redact_url(url: &Url) -> String {
    let mut clean = url.clone();
    let _ = clean.set_username("");
    let _ = clean.set_password(None);
    clean.set_query(None);
    clean.set_fragment(None);
    clean.to_string()
}

#[cfg(test)]
mod tests {
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
            include_bytes!("../tests/fixtures/self_signed_cert.der").to_vec(),
        );
        let key = rustls::pki_types::PrivateKeyDer::Pkcs8(
            include_bytes!("../tests/fixtures/self_signed_key.der")
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
}
