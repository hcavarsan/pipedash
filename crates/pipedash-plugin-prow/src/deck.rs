use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use pipedash_plugin_api::{
    check_response,
    from_status,
    map_send_error,
    redact_url,
    retry_after,
    truncate_chars,
    LogReadMode,
    LogSink,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use reqwest::header::{
    HeaderName,
    HeaderValue,
    ACCEPT,
    ACCEPT_ENCODING,
    CONTENT_ENCODING,
    CONTENT_LENGTH,
    CONTENT_TYPE,
    RANGE,
};
use reqwest::{
    RequestBuilder,
    Response,
    Url,
};

use crate::types::{
    ProwJob,
    ProwJobList,
};

const OMIT: &str = "annotations,labels,decoration_config,pod_spec";
const LIST_TIMEOUT: Duration = Duration::from_secs(120);
const LOG_TIMEOUT: Duration = Duration::from_secs(900);
const PROBE_JOB: &str = "pipedash-credential-check";
const BODY_PREVIEW_CHARS: usize = 300;

const SIGNED_OUT: &str =
    "Deck answered with a sign-in page instead of data. Check the token, or sign in again.";
const RERUN_DISABLED: &str = "This Deck doesn't start jobs directly. Run Deck with \
                              --rerun-creates-job, or add a kubeconfig in the provider settings \
                              to start jobs through Kubernetes.";
const ACTION_DENIED: &str = "Deck refused to start or abort this job. Deck only accepts its own \
                             GitHub login, or rerun_auth_configs with allow_anyone. Add a \
                             kubeconfig in the provider settings to manage ProwJobs through \
                             Kubernetes instead.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RerunMode {
    Latest,
    Original,
}

pub(crate) struct DeckClient {
    http: Arc<reqwest::Client>,
    base: Url,
    auth: Option<(HeaderName, HeaderValue)>,
    retry_policy: RetryPolicy,
}

impl DeckClient {
    pub(crate) fn new(
        http: Arc<reqwest::Client>, base: Url, auth: Option<(HeaderName, HeaderValue)>,
    ) -> Self {
        Self {
            http,
            base,
            auth,
            retry_policy: RetryPolicy::default(),
        }
    }

    pub(crate) fn base(&self) -> &Url {
        &self.base
    }

    pub(crate) fn host(&self) -> &str {
        self.base.host_str().unwrap_or("prow")
    }

    fn endpoint(&self, path: &str) -> PluginResult<Url> {
        Url::parse(&format!(
            "{}/{path}",
            self.base.as_str().trim_end_matches('/')
        ))
        .map_err(|e| PluginError::InvalidConfig(format!("Invalid Deck URL: {e}")))
    }

    pub(crate) fn authorize(&self, url: &Url, request: RequestBuilder) -> RequestBuilder {
        match &self.auth {
            Some((name, value)) if url.origin() == self.base.origin() => {
                request.header(name.clone(), value.clone())
            }
            _ => request,
        }
    }

    pub(crate) async fn check_access(&self) -> PluginResult<()> {
        let mut url = self.endpoint("badge.svg")?;
        url.query_pairs_mut().append_pair("jobs", PROBE_JOB);
        let url = &url;
        let content_type = self
            .retry_policy
            .retry(|| async move {
                let response = self
                    .authorize(url, self.http.get(url.clone()))
                    .send()
                    .await
                    .map_err(|e| map_send_error(e, url))?;
                Ok(content_type(&check_response(response).await?))
            })
            .await?;

        if content_type.contains("image/svg") {
            Ok(())
        } else if content_type.contains("text/html") {
            Err(PluginError::AuthenticationFailed(SIGNED_OUT.to_string()))
        } else {
            Err(PluginError::ApiError(format!(
                "{} doesn't look like a Prow Deck",
                redact_url(&self.base)
            )))
        }
    }

    pub(crate) async fn jobs(
        &self, org: Option<&str>, repo: Option<&str>,
    ) -> PluginResult<Vec<ProwJob>> {
        let mut url = self.endpoint("prowjobs.js")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("omit", OMIT);
            if let Some(org) = org {
                query.append_pair("org", org);
            }
            if let Some(repo) = repo {
                query.append_pair("repo", repo);
            }
        }
        let url = &url;
        let (gzip, body) = self
            .retry_policy
            .retry(|| async move {
                let response = self
                    .authorize(url, self.http.get(url.clone()))
                    .header(ACCEPT, "application/json")
                    .header(ACCEPT_ENCODING, "gzip")
                    .timeout(LIST_TIMEOUT)
                    .send()
                    .await
                    .map_err(|e| map_send_error(e, url))?;
                let response = check_response(response).await?;
                reject_sign_in_page(&response)?;
                let gzip = response
                    .headers()
                    .get(CONTENT_ENCODING)
                    .and_then(|value| value.to_str().ok())
                    .is_some_and(|value| value.trim().eq_ignore_ascii_case("gzip"));
                let body = response.bytes().await.map_err(|e| map_send_error(e, url))?;
                Ok((gzip, body))
            })
            .await?;

        tokio::task::spawn_blocking(move || decode_jobs(gzip, &body))
            .await
            .map_err(|e| PluginError::Internal(format!("Decoding Deck's job list failed: {e}")))?
    }

    pub(crate) async fn rerun(&self, prowjob: &str, mode: RerunMode) -> PluginResult<()> {
        let mut url = self.endpoint("rerun")?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("prowjob", prowjob);
            if mode == RerunMode::Latest {
                query.append_pair("mode", "latest");
            }
        }
        self.post_action(&url).await
    }

    pub(crate) async fn abort(&self, prowjob: &str) -> PluginResult<()> {
        let mut url = self.endpoint("abort")?;
        url.query_pairs_mut().append_pair("prowjob", prowjob);
        self.post_action(&url).await
    }

    async fn post_action(&self, url: &Url) -> PluginResult<()> {
        let response = self
            .retry_policy
            .retry_unsent(url, || {
                self.authorize(url, self.http.post(url.clone()))
                    .header(CONTENT_LENGTH, "0")
                    .send()
            })
            .await?;
        action_outcome(response).await
    }

    pub(crate) async fn stream_pod_log(
        &self, job: &str, build_id: &str, offset: Option<u64>, sink: &mut LogSink,
    ) -> PluginResult<(LogReadMode, u64)> {
        let mut url = self.endpoint("log")?;
        url.query_pairs_mut()
            .append_pair("job", job)
            .append_pair("id", build_id);
        let url = &url;
        sink.pipe_resumed(offset, |from| async move {
            let response = self
                .retry_policy
                .retry_unsent(url, || {
                    let request = self
                        .authorize(url, self.http.get(url.clone()))
                        .header(ACCEPT, "text/plain")
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
                    "Deck no longer has the pod log of build {build_id}"
                )));
            }
            if response.status().is_success() {
                reject_sign_in_page(&response)?;
            }
            Ok(response)
        })
        .await
    }
}

fn content_type(response: &Response) -> String {
    response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

pub(crate) fn is_html(response: &Response) -> bool {
    content_type(response).contains("text/html")
}

fn reject_sign_in_page(response: &Response) -> PluginResult<()> {
    if is_html(response) {
        return Err(PluginError::AuthenticationFailed(SIGNED_OUT.to_string()));
    }
    Ok(())
}

fn decode_jobs(gzip: bool, body: &[u8]) -> PluginResult<Vec<ProwJob>> {
    let list: Result<ProwJobList, serde_json::Error> = if gzip {
        serde_json::from_reader(flate2::read::GzDecoder::new(body))
    } else {
        serde_json::from_slice(body)
    };
    list.map(|list| list.items)
        .map_err(|e| PluginError::SerializationError(format!("Couldn't read Deck's job list: {e}")))
}

async fn action_outcome(response: Response) -> PluginResult<()> {
    let status = response.status().as_u16();
    if response.status().is_success() {
        return reject_sign_in_page(&response);
    }
    let url = response.url().clone();
    let retry = retry_after(response.headers(), Utc::now());
    let body = response.text().await.map_err(|e| map_send_error(e, &url))?;
    let preview = truncate_chars(body.trim(), BODY_PREVIEW_CHARS);

    Err(match status {
        401 | 403 => PluginError::PermissionDenied(format!("{ACTION_DENIED} Deck said: {preview}")),
        405 => PluginError::NotSupported(RERUN_DISABLED.to_string()),
        400 => PluginError::ApiError(format!("Deck refused: {preview}")),
        404 => {
            PluginError::PipelineNotFound(format!("Deck no longer lists this ProwJob: {preview}"))
        }
        _ => from_status(
            status,
            format!("{status} from {}: {preview}", redact_url(&url)),
            retry,
        ),
    })
}
