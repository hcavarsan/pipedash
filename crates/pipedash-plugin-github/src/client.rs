use chrono::Utc;
use octocrab::Octocrab;
use pipedash_plugin_api::{
    http,
    PermissionStatus,
    PluginError,
    PluginResult,
    RateLimit,
    RetryPolicy,
};
use reqwest::header::RETRY_AFTER;
use secrecy::SecretString;

use crate::permissions::PermissionChecker;

mod actions;
mod catalog;
mod runs;
mod workflows;

pub(crate) fn map_octocrab_error(context: &str, error: octocrab::Error) -> PluginError {
    match error {
        octocrab::Error::GitHub { source, .. } => {
            let status = source.status_code.as_u16();
            let rate_limited = status == 429
                || (status == 403 && source.message.to_ascii_lowercase().contains("rate limit"));
            let retry_after = source
                .headers
                .as_ref()
                .filter(|headers| rate_limited || headers.contains_key(RETRY_AFTER))
                .and_then(|headers| http::retry_after(headers, Utc::now()));
            let message = format!("{context}: {status} {}", source.message);

            if rate_limited {
                PluginError::RateLimited {
                    message,
                    retry_after,
                }
            } else {
                http::from_status(status, message, retry_after)
            }
        }
        octocrab::Error::Hyper { .. }
        | octocrab::Error::Reqwest { .. }
        | octocrab::Error::Service { .. } => {
            PluginError::NetworkError(format!("{context}: {error}"))
        }
        _ => PluginError::ApiError(format!("{context}: {error}")),
    }
}

pub(crate) struct GitHubClient {
    pub(crate) octocrab: Octocrab,
    pub(crate) retry_policy: RetryPolicy,
    permission_checker: PermissionChecker,
    http: reqwest::Client,
    token: SecretString,
    api_url: String,
}

impl GitHubClient {
    pub fn new(
        octocrab: Octocrab, http_client: reqwest::Client, token: String, api_url: &str,
    ) -> PluginResult<Self> {
        let token_secret: SecretString = token.into();
        let permission_checker = PermissionChecker::new(
            octocrab.clone(),
            http_client.clone(),
            token_secret.clone(),
            api_url,
        )?;

        Ok(Self {
            octocrab,
            retry_policy: RetryPolicy::default(),
            permission_checker,
            http: http_client,
            token: token_secret,
            api_url: api_url.trim_end_matches('/').to_string(),
        })
    }

    pub async fn fetch_rate_limit(&self) -> PluginResult<RateLimit> {
        let rate = self
            .retry_policy
            .retry(|| async {
                self.octocrab
                    .ratelimit()
                    .get()
                    .await
                    .map(|limits| limits.resources.core)
                    .map_err(|e| map_octocrab_error("Failed to fetch rate limit", e))
            })
            .await?;
        let reset_at = chrono::DateTime::from_timestamp(rate.reset as i64, 0).ok_or_else(|| {
            PluginError::ApiError(format!("Invalid rate limit reset time: {}", rate.reset))
        })?;

        Ok(RateLimit {
            limit: rate.limit as u64,
            remaining: rate.remaining as u64,
            reset_at,
        })
    }

    pub async fn check_token_permissions(
        &self, repositories: &[String],
    ) -> PluginResult<PermissionStatus> {
        self.permission_checker
            .check_token_permissions(repositories)
            .await
    }
}

#[cfg(test)]
mod tests;
