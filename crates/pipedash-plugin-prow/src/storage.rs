use std::sync::Arc;
use std::time::Duration;

use futures::{
    StreamExt,
    TryStreamExt,
};
use pipedash_plugin_api::{
    check_response,
    map_send_error,
    redact_url,
    LogReadMode,
    LogSink,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use reqwest::header::RANGE;
use reqwest::Url;
use serde::de::DeserializeOwned;

use crate::config::Bucket;
use crate::deck::{
    is_html,
    DeckClient,
};
use crate::types::{
    run_number_of,
    snowflake_at,
    snowflake_span,
    Finished,
    ObjectList,
    Started,
};

const GCS_URL: &str = "https://storage.googleapis.com";
const BUILD_LOG: &str = "build-log.txt";
const MIN_LISTING_FLOOR: i64 = 1_000_000_000_000_000_000;
const STORAGE_SIGNED_OUT: &str = "Artifact storage answered with a sign-in page instead of the \
                                  build log. Check the artifact storage URL and the token.";
const CONCURRENCY: usize = 8;
const LOG_TIMEOUT: Duration = Duration::from_secs(900);

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
    base_override: Option<Url>,
    retry_policy: RetryPolicy,
}

impl StorageClient {
    pub(crate) fn new(http: Arc<reqwest::Client>, base_override: Option<Url>) -> Self {
        Self {
            http,
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
        &self, deck: &DeckClient, artifacts: &Artifacts, offset: Option<u64>, sink: &mut LogSink,
    ) -> PluginResult<(LogReadMode, u64)> {
        let url = self.log_url(artifacts).ok_or_else(|| {
            PluginError::PipelineNotFound(format!(
                "No artifact storage is set up for {} buckets",
                artifacts.provider
            ))
        })?;
        let url = &url;
        sink.pipe_resumed(offset, |from| async move {
            let response = self
                .retry_policy
                .retry_unsent(url, || {
                    let request = deck
                        .authorize(url, self.http.get(url.clone()))
                        .timeout(LOG_TIMEOUT);
                    match from {
                        Some(from) => request.header(RANGE, format!("bytes={from}-")),
                        None => request,
                    }
                    .send()
                })
                .await?;

            if matches!(response.status().as_u16(), 404 | 410) {
                return Err(PluginError::PipelineNotFound(format!(
                    "Storage has no build log at {}",
                    redact_url(url)
                )));
            }
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
                    .http
                    .get(url.clone())
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
                let body = response.bytes().await.map_err(|e| map_send_error(e, url))?;
                Ok(Some(body.to_vec()))
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

    pub(crate) async fn build_numbers(
        &self, bucket: &Bucket, root: HistoryRoot, job: &str, from: Option<i64>, until: Option<i64>,
    ) -> PluginResult<Vec<i64>> {
        let Some(base) = self
            .base(&bucket.provider)
            .filter(|_| bucket.provider == "gs")
        else {
            return Ok(Vec::new());
        };
        let list_url = Url::parse(&format!("{base}/storage/v1/b/{}/o", bucket.name))
            .map_err(|e| PluginError::InvalidConfig(format!("Invalid storage URL: {e}")))?;
        let prefix = format!("{}/", root.key(job));
        let lexical = from.is_some_and(|from| from >= MIN_LISTING_FLOOR);
        let offset = |bound: Option<i64>| {
            bound
                .filter(|_| lexical)
                .map(|bound| format!("{prefix}{bound}"))
        };
        let (start_offset, end_offset) = (offset(from), offset(until));
        let mut numbers: Vec<i64> = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let mut url = list_url.clone();
            {
                let mut query = url.query_pairs_mut();
                query
                    .append_pair("prefix", &prefix)
                    .append_pair("maxResults", "1000");
                match root {
                    HistoryRoot::Logs => query
                        .append_pair("delimiter", "/")
                        .append_pair("fields", "prefixes,nextPageToken"),
                    HistoryRoot::PullDirectory => {
                        query.append_pair("fields", "items(name),nextPageToken")
                    }
                };
                if let Some(start) = &start_offset {
                    query.append_pair("startOffset", start);
                }
                if let Some(end) = &end_offset {
                    query.append_pair("endOffset", end);
                }
                if let Some(token) = &page_token {
                    query.append_pair("pageToken", token);
                }
            }
            let Some(list) = self.read_json::<ObjectList>(url).await? else {
                break;
            };
            match root {
                HistoryRoot::Logs => numbers.extend(
                    list.prefixes
                        .iter()
                        .filter_map(|prefix| prefix.trim_end_matches('/').rsplit('/').next())
                        .filter_map(|name| name.parse::<i64>().ok()),
                ),
                HistoryRoot::PullDirectory => numbers.extend(
                    list.items
                        .iter()
                        .filter_map(|item| item.name.strip_suffix(".txt"))
                        .filter_map(|name| name.rsplit('/').next())
                        .filter_map(|name| name.parse::<i64>().ok()),
                ),
            }
            match list.next_page_token {
                Some(token) if !token.is_empty() => page_token = Some(token),
                _ => break,
            }
        }

        numbers.retain(|number| {
            *number > 0
                && from.is_none_or(|from| *number >= from)
                && until.is_none_or(|until| *number < until)
        });
        numbers.sort_unstable_by(|a, b| b.cmp(a));
        numbers.dedup();
        Ok(numbers)
    }

    pub(crate) async fn find_build(
        &self, bucket: &Bucket, root: HistoryRoot, job: &str, run_number: i64,
    ) -> PluginResult<Option<StoredBuild>> {
        let span = snowflake_span(run_number);
        let bounded = span.is_some_and(|(start, _)| start >= MIN_LISTING_FLOOR);
        if bounded {
            if let Some(build_id) = self
                .listed_build(bucket, root, job, run_number, span)
                .await?
            {
                return self.build(bucket, root, job, build_id).await;
            }
        }
        if run_number_of(run_number) == run_number {
            if let Some(build) = self.build(bucket, root, job, run_number).await? {
                return Ok(Some(build));
            }
        }
        if bounded {
            return Ok(None);
        }
        match self
            .listed_build(bucket, root, job, run_number, span)
            .await?
        {
            Some(build_id) => self.build(bucket, root, job, build_id).await,
            None => Ok(None),
        }
    }

    async fn listed_build(
        &self, bucket: &Bucket, root: HistoryRoot, job: &str, run_number: i64,
        span: Option<(i64, i64)>,
    ) -> PluginResult<Option<i64>> {
        let Some((start, end)) = span else {
            return Ok(None);
        };
        Ok(self
            .build_numbers(bucket, root, job, Some(start), Some(end))
            .await?
            .into_iter()
            .find(|build_id| run_number_of(*build_id) == run_number))
    }

    pub(crate) async fn build(
        &self, bucket: &Bucket, root: HistoryRoot, job: &str, number: i64,
    ) -> PluginResult<Option<StoredBuild>> {
        let directory = match root {
            HistoryRoot::Logs => format!("{}/{number}", root.key(job)),
            HistoryRoot::PullDirectory => {
                let Some(url) = self.object_url(
                    &bucket.provider,
                    &bucket.name,
                    &format!("{}/{number}.txt", root.key(job)),
                ) else {
                    return Ok(None);
                };
                let Some(link) = self.read_object(url).await? else {
                    return Ok(None);
                };
                let link = String::from_utf8_lossy(&link);
                let Some(directory) = link
                    .trim()
                    .strip_prefix(&format!("{}://{}/", bucket.provider, bucket.name))
                else {
                    return Ok(None);
                };
                directory.trim_end_matches('/').to_string()
            }
        };
        let (Some(started_url), Some(finished_url)) = (
            self.object_url(
                &bucket.provider,
                &bucket.name,
                &format!("{directory}/started.json"),
            ),
            self.object_url(
                &bucket.provider,
                &bucket.name,
                &format!("{directory}/finished.json"),
            ),
        ) else {
            return Ok(None);
        };
        let (started, finished) = futures::try_join!(
            self.read_json::<Started>(started_url),
            self.read_json::<Finished>(finished_url)
        )?;
        let (Some(started), Some(finished)) = (started, finished) else {
            return Ok(None);
        };
        Ok(Some(StoredBuild {
            number,
            artifacts: Artifacts {
                provider: bucket.provider.clone(),
                bucket: bucket.name.clone(),
                path: directory,
            },
            started,
            finished,
        }))
    }

    pub(crate) async fn latest_build(
        &self, bucket: &Bucket, root: HistoryRoot, job: &str,
    ) -> PluginResult<Option<StoredBuild>> {
        let Some(url) = self.object_url(
            &bucket.provider,
            &bucket.name,
            &format!("{}/latest-build.txt", root.key(job)),
        ) else {
            return Ok(None);
        };
        let Some(body) = self.read_object(url).await? else {
            return Ok(None);
        };
        match String::from_utf8_lossy(&body).trim().parse::<i64>() {
            Ok(number) if number > 0 => self.build(bucket, root, job, number).await,
            _ => Ok(None),
        }
    }

    pub(crate) async fn builds(
        &self, bucket: &Bucket, root: HistoryRoot, job: &str, numbers: Vec<i64>,
    ) -> PluginResult<Vec<StoredBuild>> {
        let builds: Vec<Option<StoredBuild>> = futures::stream::iter(numbers)
            .map(|number| self.build(bucket, root, job, number))
            .buffered(CONCURRENCY)
            .try_collect()
            .await?;
        Ok(builds.into_iter().flatten().collect())
    }
}
