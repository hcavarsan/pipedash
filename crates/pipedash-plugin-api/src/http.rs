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
use crate::utils::RetryPolicy;

const MAX_RETRY_AFTER: Duration = Duration::from_secs(3600);
const EPOCH_THRESHOLD: u64 = 1_000_000_000;
const BODY_PREVIEW_CHARS: usize = 300;
const BODY_PREVIEW_BYTES: usize = BODY_PREVIEW_CHARS * 4;

pub async fn read_prefix(mut resp: reqwest::Response, limit: usize) -> reqwest::Result<Vec<u8>> {
    let mut body = Vec::new();
    while body.len() < limit {
        let Some(chunk) = resp.chunk().await? else {
            break;
        };
        let take = chunk.len().min(limit - body.len());
        body.extend_from_slice(&chunk[..take]);
    }
    Ok(body)
}

pub async fn body_preview(resp: reqwest::Response) -> String {
    let body = read_prefix(resp, BODY_PREVIEW_BYTES)
        .await
        .unwrap_or_default();
    truncate_chars(String::from_utf8_lossy(&body).trim(), BODY_PREVIEW_CHARS).into_owned()
}

pub async fn check_response(resp: reqwest::Response) -> PluginResult<reqwest::Response> {
    let status = resp.status();
    if status.is_success() || status.is_redirection() {
        return Ok(resp);
    }

    let url = redact_url(resp.url());
    let headers = resp.headers().clone();
    let body = body_preview(resp).await;
    let message = format!("{} from {}: {}", status.as_u16(), url, body);
    Err(from_response_parts(
        status.as_u16(),
        &headers,
        message,
        Utc::now(),
    ))
}

pub fn from_response_parts(
    status: u16, headers: &HeaderMap, message: String, now: DateTime<Utc>,
) -> PluginError {
    let retry_after = retry_after(headers, now);
    let rate_limit_exhausted = headers
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.trim() == "0");
    let has_retry_after = headers.contains_key(reqwest::header::RETRY_AFTER);

    if status == 403 && (rate_limit_exhausted || has_retry_after) {
        return PluginError::RateLimited {
            message,
            retry_after,
        };
    }

    from_status(status, message, retry_after)
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

pub fn require_client(
    client: Option<std::sync::Arc<reqwest::Client>>,
) -> PluginResult<std::sync::Arc<reqwest::Client>> {
    client.ok_or_else(|| PluginError::Internal("HTTP client was not provided".into()))
}

pub fn sensitive_header(value: &str, label: &str) -> PluginResult<reqwest::header::HeaderValue> {
    let mut header = reqwest::header::HeaderValue::from_str(value)
        .map_err(|_| PluginError::InvalidConfig(format!("Invalid {label} header")))?;
    header.set_sensitive(true);
    Ok(header)
}

pub fn parse_http_url(raw: &str, label: &str) -> PluginResult<Url> {
    let url = Url::parse(raw)
        .map_err(|e| PluginError::InvalidConfig(format!("Invalid {label} URL: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(PluginError::InvalidConfig(format!(
            "{label} URL must be an HTTP or HTTPS URL with a host"
        )));
    }
    Ok(url)
}

pub async fn decode_json<T: serde::de::DeserializeOwned>(
    response: reqwest::Response, context: &str,
) -> PluginResult<T> {
    let url = redact_url(response.url());
    response.json().await.map_err(|e| {
        PluginError::SerializationError(format!(
            "Failed to parse {context} from {url}: {}",
            e.without_url()
        ))
    })
}

pub fn api_url(base: &str, path: &str, query: &[(&str, String)], label: &str) -> PluginResult<Url> {
    parse_http_url(&format!("{base}{path}"), label).map(|url| with_query(url, query))
}

pub fn with_query(mut url: Url, query: &[(&str, String)]) -> Url {
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    url
}

pub fn bearer_request(
    http: &reqwest::Client, method: reqwest::Method, url: &Url, token: &str,
) -> reqwest::RequestBuilder {
    http.request(method, url.clone()).bearer_auth(token)
}

pub async fn send(url: &Url, request: reqwest::RequestBuilder) -> PluginResult<reqwest::Response> {
    request.send().await.map_err(|e| map_send_error(e, url))
}

pub async fn get_json<T: serde::de::DeserializeOwned>(
    retry: &RetryPolicy, url: &Url, context: &str, build: impl Fn() -> reqwest::RequestBuilder,
) -> PluginResult<T> {
    retry
        .retry(|| async {
            let response = send(url, build()).await?;
            decode_json(check_response(response).await?, context).await
        })
        .await
}

pub async fn send_checked(
    retry: &RetryPolicy, url: &Url, build: impl Fn() -> reqwest::RequestBuilder,
) -> PluginResult<reqwest::Response> {
    check_response(retry.retry_unsent(url, || build().send()).await?).await
}

#[cfg(test)]
mod tests;
