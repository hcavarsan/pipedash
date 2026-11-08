use std::time::Duration;

use reqwest::header::RANGE;
use reqwest::{
    RequestBuilder,
    Response,
    Url,
};

use crate::error::{
    PluginError,
    PluginResult,
};
use crate::utils::RetryPolicy;

pub const LOG_REQUEST_TIMEOUT: Duration = Duration::from_secs(900);

pub async fn send_log(
    retry: &RetryPolicy, url: &Url, from: Option<u64>, gone: &str,
    build: impl Fn() -> RequestBuilder,
) -> PluginResult<Response> {
    let response = retry
        .retry_unsent(url, || {
            let request = build().timeout(LOG_REQUEST_TIMEOUT);
            match from {
                Some(from) => request.header(RANGE, format!("bytes={from}-")),
                None => request,
            }
            .send()
        })
        .await?;

    if matches!(response.status().as_u16(), 404 | 410) {
        return Err(PluginError::PipelineNotFound(gone.to_string()));
    }
    Ok(response)
}

#[cfg(test)]
mod tests;
