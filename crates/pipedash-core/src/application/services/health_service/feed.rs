use std::time::Duration;

use chrono::Utc;
use pipedash_plugin_api::{
    retry_after,
    FeedFormat,
};
use reqwest::header::{
    ACCEPT,
    ETAG,
    IF_NONE_MATCH,
};
use reqwest::{
    Client,
    StatusCode,
    Url,
};

use crate::infrastructure::status_feeds::{
    parse_statusio,
    parse_statuspage,
    ParsedFeed,
    MAX_FEED_BYTES,
};

pub(super) const FEED_UNREACHABLE: &str = "Couldn't reach the status page";
pub(super) const FEED_INVALID: &str = "The status page returned an invalid response";

#[derive(Debug)]
pub(super) enum FeedOutcome {
    Parsed {
        feed: Box<ParsedFeed>,
        etag: Option<String>,
    },
    NotModified,
    Failed {
        message: &'static str,
        retry_after: Option<Duration>,
    },
}

fn failed(message: &'static str) -> FeedOutcome {
    FeedOutcome::Failed {
        message,
        retry_after: None,
    }
}

pub(super) async fn fetch_feed(
    client: &Client, url: &Url, format: FeedFormat, etag: Option<&str>,
) -> FeedOutcome {
    let mut request = client.get(url.clone()).header(ACCEPT, "application/json");
    if let Some(etag) = etag {
        request = request.header(IF_NONE_MATCH, etag);
    }
    let mut response = match request.send().await {
        Ok(response) => response,
        Err(e) => {
            tracing::debug!(error = %e.without_url(), "Status feed request failed");
            return failed(FEED_UNREACHABLE);
        }
    };

    let status = response.status();
    if status == StatusCode::NOT_MODIFIED {
        return FeedOutcome::NotModified;
    }
    if !status.is_success() {
        tracing::debug!(%status, "Status feed returned an error status");
        return FeedOutcome::Failed {
            message: FEED_UNREACHABLE,
            retry_after: retry_after(response.headers(), Utc::now()),
        };
    }

    let etag = response
        .headers()
        .get(ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let mut body = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) if body.len() + chunk.len() > MAX_FEED_BYTES => {
                tracing::debug!("Status feed is over the size limit");
                return failed(FEED_INVALID);
            }
            Ok(Some(chunk)) => body.extend_from_slice(&chunk),
            Ok(None) => break,
            Err(e) => {
                tracing::debug!(error = %e.without_url(), "Status feed body failed");
                return failed(FEED_UNREACHABLE);
            }
        }
    }

    let parsed = match format {
        FeedFormat::Statuspage => parse_statuspage(&body),
        FeedFormat::StatusIo => parse_statusio(&body),
        _ => return failed(FEED_INVALID),
    };
    match parsed {
        Ok(feed) => FeedOutcome::Parsed {
            feed: Box::new(feed),
            etag,
        },
        Err(e) => {
            tracing::debug!(error = %e, "Status feed could not be parsed");
            failed(FEED_INVALID)
        }
    }
}
