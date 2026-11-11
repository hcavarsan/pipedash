use std::collections::HashMap;
use std::future::Future;
use std::sync::OnceLock;
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use futures::StreamExt;
use reqwest::Url;

use crate::config::comma_separated;
use crate::http::map_send_error;
use crate::{
    PipelineStatus,
    PluginError,
    PluginResult,
};

pub struct RetryPolicy {
    pub max_retries: usize,
    pub initial_delay: Duration,
    pub exponential_backoff: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            exponential_backoff: true,
        }
    }
}

impl RetryPolicy {
    pub fn new(max_retries: usize, initial_delay: Duration, exponential_backoff: bool) -> Self {
        Self {
            max_retries,
            initial_delay,
            exponential_backoff,
        }
    }

    pub async fn retry<F, Fut, T>(&self, operation: F) -> PluginResult<T>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = PluginResult<T>>,
    {
        let mut delay = self.initial_delay;
        let mut attempt = 1;

        loop {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) if attempt < self.max_retries && Self::should_retry(&e, delay) => {
                    tokio::time::sleep(delay).await;
                    delay = self.next_delay(delay);
                    attempt += 1;
                }
                Err(e) => return Err(e),
            }
        }
    }

    pub async fn retry_unsent<F, Fut>(&self, url: &Url, send: F) -> PluginResult<reqwest::Response>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<reqwest::Response, reqwest::Error>>,
    {
        let mut delay = self.initial_delay;
        let mut attempt = 1;

        loop {
            match send().await {
                Ok(response) => return Ok(response),
                Err(e) if attempt < self.max_retries && e.is_connect() && !e.is_timeout() => {
                    tokio::time::sleep(delay).await;
                    delay = self.next_delay(delay);
                    attempt += 1;
                }
                Err(e) => return Err(map_send_error(e, url)),
            }
        }
    }

    fn should_retry(error: &PluginError, delay: Duration) -> bool {
        error.is_transient() && error.retry_after().is_none_or(|hint| hint <= delay)
    }

    fn next_delay(&self, delay: Duration) -> Duration {
        if self.exponential_backoff {
            delay.saturating_mul(2)
        } else {
            delay
        }
    }
}

#[derive(Debug)]
pub struct Cached<T>(OnceLock<T>);

impl<T> Default for Cached<T> {
    fn default() -> Self {
        Self(OnceLock::new())
    }
}

impl<T: Clone> Cached<T> {
    pub async fn get_or_fetch<F, Fut>(&self, fetch: F) -> PluginResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = PluginResult<T>>,
    {
        if let Some(value) = self.0.get() {
            return Ok(value.clone());
        }
        let value = fetch().await?;
        let _ = self.0.set(value.clone());
        Ok(value)
    }
}

pub async fn retain_with_ci<F, Fut>(ids: Vec<String>, has_ci: F) -> Vec<String>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = PluginResult<bool>>,
{
    futures::stream::iter(ids)
        .map(|id| {
            let check = has_ci(id.clone());
            async move { check.await.unwrap_or(true).then_some(id) }
        })
        .buffered(8)
        .filter_map(std::future::ready)
        .collect()
        .await
}

pub fn status_from_conclusion(
    status: &str, conclusion: Option<&str>, successful: &[&str], cancelled: &[&str],
) -> PipelineStatus {
    match (status, conclusion) {
        ("completed", Some(c)) if successful.contains(&c) => PipelineStatus::Success,
        ("completed", Some(c)) if cancelled.contains(&c) => PipelineStatus::Cancelled,
        ("completed", Some("skipped")) => PipelineStatus::Skipped,
        ("completed", _) => PipelineStatus::Failed,
        ("in_progress", _) | ("queued", _) => PipelineStatus::Running,
        _ => PipelineStatus::Pending,
    }
}

pub fn deployed_status(result: PipelineStatus, live: PipelineStatus) -> PipelineStatus {
    match (result, live) {
        (PipelineStatus::Success, PipelineStatus::Degraded | PipelineStatus::Failed) => {
            PipelineStatus::Degraded
        }
        _ => result,
    }
}

pub fn repository_name(repo_url: &str) -> String {
    let cleaned = repo_url
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .trim_end_matches('/');

    if !cleaned.contains("://") {
        if let Some((_, path)) = cleaned.rsplit_once(':') {
            if path.contains('/') {
                return path.to_string();
            }
        }
    }

    let mut segments = cleaned.rsplit('/');
    match (segments.next(), segments.next()) {
        (Some(repo), Some(org)) => format!("{org}/{repo}"),
        _ => repo_url.to_string(),
    }
}

pub fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

pub fn duration_seconds(
    started: Option<DateTime<Utc>>, finished: Option<DateTime<Utc>>,
) -> Option<i64> {
    Some((finished? - started?).num_seconds())
}

pub fn collect_partial<T>(
    results: impl IntoIterator<Item = PluginResult<T>>, mut skipped: impl FnMut(&PluginError),
) -> PluginResult<Vec<T>> {
    let mut items = Vec::new();
    for result in results {
        match result {
            Ok(item) => items.push(item),
            Err(error) if error.is_transient() => return Err(error),
            Err(error) => skipped(&error),
        }
    }
    Ok(items)
}

pub fn selected_items(config: &HashMap<String, String>) -> impl Iterator<Item = &str> {
    config
        .get("selected_items")
        .into_iter()
        .flat_map(|items| comma_separated(items))
}

pub const fn provider_type(package_name: &'static str) -> &'static str {
    const PREFIX: &[u8] = b"pipedash-plugin-";
    let name = package_name.as_bytes();
    if name.len() < PREFIX.len() {
        return package_name;
    }
    let (head, rest) = name.split_at(PREFIX.len());
    let mut index = 0;
    while index < PREFIX.len() {
        if head[index] != PREFIX[index] {
            return package_name;
        }
        index += 1;
    }
    match core::str::from_utf8(rest) {
        Ok(stripped) => stripped,
        Err(_) => package_name,
    }
}

pub fn svg_data_url(svg: &str) -> String {
    const PREFIX: &str = "data:image/svg+xml,";
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut url = String::with_capacity(PREFIX.len() + svg.len());
    url.push_str(PREFIX);
    for byte in svg.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/:;,=!*'()".contains(&byte) {
            url.push(char::from(byte));
        } else {
            url.push('%');
            url.push(char::from(HEX[usize::from(byte >> 4)]));
            url.push(char::from(HEX[usize::from(byte & 0x0F)]));
        }
    }
    url
}

#[cfg(test)]
mod tests;
