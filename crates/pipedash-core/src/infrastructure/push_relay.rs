use std::time::Duration;

use pipedash_plugin_api::ClientOptions;
use serde::Serialize;

use super::HttpClientManager;

const RELAY_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayOutcome {
    Delivered,
    Gone,
    RateLimited,
    Failed,
}

pub(crate) fn relay_outcome(status: u16) -> RelayOutcome {
    match status {
        202 => RelayOutcome::Delivered,
        404 | 410 => RelayOutcome::Gone,
        429 => RelayOutcome::RateLimited,
        _ => RelayOutcome::Failed,
    }
}

#[derive(Serialize)]
pub(crate) struct RelayNotification<'a> {
    pub(crate) subscription_id: &'a str,
    pub(crate) title: &'a str,
    pub(crate) body: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) thread_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pipeline_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) provider_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) run_number: Option<u64>,
}

#[derive(Serialize)]
struct RelayAlert<'a> {
    subscription_id: &'a str,
    alert_id: &'a str,
}

pub fn normalize_relay_url(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');

    (!url.is_empty()).then(|| url.to_owned())
}

pub(crate) async fn post_relay<T: Serialize>(
    http: &HttpClientManager, relay_url: &str, body: &T,
) -> RelayOutcome {
    let client = match http.client_for(&ClientOptions::default()) {
        Ok(client) => client,
        Err(error) => {
            tracing::warn!(%error, "Couldn't build the push relay client");
            return RelayOutcome::Failed;
        }
    };

    let response = client
        .post(format!("{relay_url}/v1/notifications"))
        .timeout(RELAY_TIMEOUT)
        .json(body)
        .send()
        .await;

    match response {
        Ok(response) => relay_outcome(response.status().as_u16()),
        Err(error) => {
            tracing::warn!(error = %error.without_url(), "Push relay request failed");
            RelayOutcome::Failed
        }
    }
}

pub(crate) async fn send_alert(
    http: &HttpClientManager, relay_url: &str, subscription_id: &str, alert_id: &str,
) -> RelayOutcome {
    post_relay(
        http,
        relay_url,
        &RelayAlert {
            subscription_id,
            alert_id,
        },
    )
    .await
}

#[cfg(test)]
mod tests;
