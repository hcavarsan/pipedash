use std::sync::Arc;

use pipedash_plugin_api::logs::fetch::send_log;
use pipedash_plugin_api::{
    check_response,
    map_send_error,
    read_prefix,
    redact_url,
    LogReadMode,
    LogSink,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use reqwest::Url;
use serde::de::DeserializeOwned;

use crate::deck::{
    is_html,
    DeckClient,
};
use crate::types::{
    snowflake_at,
    Finished,
    Started,
};

mod history;

const GCS_URL: &str = "https://storage.googleapis.com";
const BUILD_LOG: &str = "build-log.txt";
const MIN_LISTING_FLOOR: i64 = 1_000_000_000_000_000_000;
const STORAGE_SIGNED_OUT: &str = concat!(
    "Artifact storage answered with a sign-in page instead of the ",
    "build log. Check the artifact storage URL and the token."
);
const MAX_OBJECT_BYTES: usize = 16 << 20;

pub(crate) fn listing_floor(unix_ms: i64) -> Option<i64> {
    snowflake_at(unix_ms).filter(|floor| *floor >= MIN_LISTING_FLOOR)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Artifacts {
    pub provider: String,
    pub bucket: String,
    pub path: String,
}

impl Artifacts {
    pub(crate) fn from_view_url(url: &str) -> Option<Self> {
        let url = Url::parse(url).ok()?;
        let mut segments = url
            .path_segments()?
            .skip_while(|segment| *segment != "view")
            .skip(1)
            .filter(|segment| !segment.is_empty());
        let provider = segments.next()?.to_string();
        let bucket = segments.next()?.to_string();
        let path = segments.collect::<Vec<_>>().join("/");
        (!path.is_empty()).then_some(Self {
            provider,
            bucket,
            path,
        })
    }

    pub(crate) fn from_metadata(value: &str) -> Option<Self> {
        let (provider, rest) = value.split_once("://")?;
        let (bucket, path) = rest.split_once('/')?;
        let path = path.trim_end_matches('/');
        if provider.is_empty() || bucket.is_empty() || path.is_empty() {
            return None;
        }
        Some(Self {
            provider: provider.to_string(),
            bucket: bucket.to_string(),
            path: path.to_string(),
        })
    }

    pub(crate) fn to_metadata(&self) -> String {
        format!("{}://{}/{}", self.provider, self.bucket, self.path)
    }

    pub(crate) fn view_url(&self, deck: &Url) -> String {
        format!(
            "{}/view/{}/{}/{}",
            deck.as_str().trim_end_matches('/'),
            self.provider,
            self.bucket,
            self.path
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryRoot {
    Logs,
    PullDirectory,
}

impl HistoryRoot {
    pub(crate) fn for_job_type(job_type: &str) -> Self {
        match job_type {
            "presubmit" | "batch" => Self::PullDirectory,
            _ => Self::Logs,
        }
    }

    fn key(&self, job: &str) -> String {
        match self {
            Self::Logs => format!("logs/{job}"),
            Self::PullDirectory => format!("pr-logs/directory/{job}"),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct StoredBuild {
    pub number: i64,
    pub artifacts: Artifacts,
    pub started: Started,
    pub finished: Finished,
}

pub(crate) struct StorageClient {
    http: Arc<reqwest::Client>,
    deck: Arc<DeckClient>,
    base_override: Option<Url>,
    retry_policy: RetryPolicy,
}

impl StorageClient {
    pub(crate) fn new(
        http: Arc<reqwest::Client>, deck: Arc<DeckClient>, base_override: Option<Url>,
    ) -> Self {
        Self {
            http,
            deck,
            base_override,
            retry_policy: RetryPolicy::default(),
        }
    }

    fn base(&self, provider: &str) -> Option<String> {
        match &self.base_override {
            Some(url) => Some(url.as_str().trim_end_matches('/').to_string()),
            None => (provider == "gs").then(|| GCS_URL.to_string()),
        }
    }

    pub(crate) fn object_url(&self, provider: &str, bucket: &str, key: &str) -> Option<Url> {
        Url::parse(&format!("{}/{bucket}/{key}", self.base(provider)?)).ok()
    }

    pub(crate) fn log_url(&self, artifacts: &Artifacts) -> Option<Url> {
        self.object_url(
            &artifacts.provider,
            &artifacts.bucket,
            &format!("{}/{BUILD_LOG}", artifacts.path),
        )
    }

    pub(crate) async fn stream_build_log(
        &self, artifacts: &Artifacts, offset: Option<u64>, sink: &mut LogSink,
    ) -> PluginResult<(LogReadMode, u64)> {
        let url = self.log_url(artifacts).ok_or_else(|| {
            PluginError::PipelineNotFound(format!(
                "No artifact storage is set up for {} buckets",
                artifacts.provider
            ))
        })?;
        let url = &url;
        let gone = &format!("Storage has no build log at {}", redact_url(url));
        sink.pipe_resumed(offset, |from| async move {
            let response = send_log(&self.retry_policy, url, from, gone, || {
                self.deck.authorize(url, self.http.get(url.clone()))
            })
            .await?;
            if response.status().is_success() && is_html(&response) {
                return Err(PluginError::AuthenticationFailed(
                    STORAGE_SIGNED_OUT.to_string(),
                ));
            }
            Ok(response)
        })
        .await
    }

    async fn read_object(&self, url: Url) -> PluginResult<Option<Vec<u8>>> {
        let url = &url;
        self.retry_policy
            .retry(|| async move {
                let response = self
                    .deck
                    .authorize(url, self.http.get(url.clone()))
                    .send()
                    .await
                    .map_err(|e| map_send_error(e, url))?;
                if response.status() == reqwest::StatusCode::NOT_FOUND {
                    return Ok(None);
                }
                let response = check_response(response).await?;
                if is_html(&response) {
                    return Err(PluginError::AuthenticationFailed(
                        STORAGE_SIGNED_OUT.to_string(),
                    ));
                }
                let body = read_prefix(response, MAX_OBJECT_BYTES + 1)
                    .await
                    .map_err(|e| map_send_error(e, url))?;
                if body.len() > MAX_OBJECT_BYTES {
                    return Err(PluginError::ApiError(format!(
                        "Artifact storage sent {} larger than the {} MiB limit",
                        redact_url(url),
                        MAX_OBJECT_BYTES >> 20
                    )));
                }
                Ok(Some(body))
            })
            .await
    }

    async fn read_json<T: DeserializeOwned>(&self, url: Url) -> PluginResult<Option<T>> {
        let redacted = redact_url(&url);
        let Some(body) = self.read_object(url).await? else {
            return Ok(None);
        };
        serde_json::from_slice(&body)
            .map(Some)
            .map_err(|e| PluginError::SerializationError(format!("Couldn't read {redacted}: {e}")))
    }
}
