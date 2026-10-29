use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use pipedash_plugin_api::{
    check_response,
    map_send_error,
    next_log_chunk,
    redact_url,
    LogCursor,
    LogRead,
    LogSink,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use reqwest::{
    header::{
        ACCEPT,
        CONTENT_TYPE,
    },
    Method,
    StatusCode,
    Url,
};
use serde::de::DeserializeOwned;
use serde_json::{
    json,
    Value,
};

use crate::log_tail::{
    LogPosition,
    Tally,
};
use crate::types::{
    Build,
    Definition,
    DefinitionDetail,
    ListResponse,
    Project,
    Timeline,
};

const API_VERSION: &str = "7.0";
const CONTINUATION_HEADER: &str = "x-ms-continuationtoken";
const PAGE_SIZE: usize = 100;
const MAX_PAGES: usize = 100;
const LOG_TIMEOUT: Duration = Duration::from_secs(900);

pub(crate) struct AzureDevOpsClient {
    http_client: Arc<reqwest::Client>,
    base_url: Url,
    token: String,
    retry_policy: RetryPolicy,
}

impl AzureDevOpsClient {
    pub fn new(http_client: Arc<reqwest::Client>, base_url: Url, token: String) -> Self {
        Self {
            http_client,
            base_url,
            token,
            retry_policy: RetryPolicy::default(),
        }
    }

    pub fn server_url(&self) -> &Url {
        &self.base_url
    }

    pub async fn ping(&self) -> PluginResult<()> {
        self.list::<Project>(None, "_apis/projects", &[], 1, 1)
            .await
            .map(drop)
    }

    pub async fn list_projects(&self) -> PluginResult<Vec<Project>> {
        self.list(None, "_apis/projects", &[], PAGE_SIZE, usize::MAX)
            .await
    }

    pub async fn list_definitions(&self, project: &str) -> PluginResult<Vec<Definition>> {
        self.list(
            Some(project),
            "_apis/build/definitions",
            &[("includeLatestBuilds", "true".to_string())],
            PAGE_SIZE,
            usize::MAX,
        )
        .await
    }

    pub async fn has_definitions(&self, project: &str) -> PluginResult<bool> {
        let definitions: Vec<Definition> = self
            .list(Some(project), "_apis/build/definitions", &[], 1, 1)
            .await?;
        Ok(!definitions.is_empty())
    }

    pub async fn get_definition(
        &self, project: &str, definition_id: i64,
    ) -> PluginResult<DefinitionDetail> {
        let path = format!("_apis/build/definitions/{definition_id}");
        let (definition, _) = self.get_json(Some(project), &path, &[]).await?;
        Ok(definition)
    }

    pub async fn list_builds(
        &self, project: &str, definition_id: i64, limit: usize,
    ) -> PluginResult<Vec<Build>> {
        self.list(
            Some(project),
            "_apis/build/builds",
            &[
                ("definitions", definition_id.to_string()),
                ("queryOrder", "queueTimeDescending".to_string()),
            ],
            limit,
            limit,
        )
        .await
    }

    pub async fn get_build(&self, project: &str, build_id: i64) -> PluginResult<Build> {
        let path = format!("_apis/build/builds/{build_id}");
        let (build, _) = self.get_json(Some(project), &path, &[]).await?;
        Ok(build)
    }

    pub async fn get_timeline(&self, project: &str, build_id: i64) -> PluginResult<Timeline> {
        let path = format!("_apis/build/builds/{build_id}/timeline");
        let (timeline, _) = self.get_json(Some(project), &path, &[]).await?;
        Ok(timeline)
    }

    pub async fn stream_build_log(
        &self, project: &str, build_id: i64, log_id: i64, attempt: i64, from: Option<&LogCursor>,
        sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let mut resume = match from {
            None => None,
            Some(cursor) => {
                let known = cursor
                    .parse::<LogPosition>()
                    .filter(|position| position.is_for(build_id, log_id, attempt));
                if known.is_none() {
                    sink.restart()?;
                }
                known
            }
        };
        let mut response = self
            .open_build_log(
                project,
                build_id,
                log_id,
                resume.as_ref().map(LogPosition::request_line),
            )
            .await?;
        let mut head = Vec::new();
        let mut skip = 0;
        if let Some(position) = &resume {
            let mut newlines = 0;
            while newlines < 2 {
                let Some(chunk) = next_log_chunk(&mut response).await? else {
                    break;
                };
                newlines += chunk.iter().filter(|&&byte| byte == b'\n').count();
                head.extend_from_slice(&chunk);
            }
            match position.overlap(&head) {
                Some(bytes) => skip = bytes,
                None => {
                    sink.restart()?;
                    resume = None;
                    head.clear();
                    response = self.open_build_log(project, build_id, log_id, None).await?;
                }
            }
        }

        let mut tally = resume.as_ref().map_or_else(Tally::default, Tally::resume);
        let mut eol = resume.as_ref().is_some_and(|position| position.open);
        let mut chunk = Some(Bytes::from(head).slice(skip..));
        while let Some(bytes) = chunk {
            if !bytes.is_empty() {
                if std::mem::take(&mut eol) {
                    sink.write(Bytes::from_static(b"\n")).await?;
                }
                tally.feed(&bytes);
                sink.write(bytes).await?;
            }
            chunk = next_log_chunk(&mut response).await?;
        }

        let appended = resume.is_some();
        let next = match resume {
            Some(position) if !tally.fed() => position,
            _ => tally.position(build_id, log_id, attempt),
        };
        let cursor = LogCursor::new(&next)?;
        Ok(if appended {
            LogRead::append(cursor)
        } else {
            LogRead::replace(Some(cursor))
        })
    }

    async fn open_build_log(
        &self, project: &str, build_id: i64, log_id: i64, start_line: Option<u64>,
    ) -> PluginResult<reqwest::Response> {
        let path = format!("_apis/build/builds/{build_id}/logs/{log_id}");
        let query: Vec<_> = start_line
            .map(|line| ("startLine", line.to_string()))
            .into_iter()
            .collect();
        let url = self.url(Some(project), &path, &query)?;
        let response = self
            .retry_policy
            .retry_unsent(&url, || {
                self.authorized(Method::GET, &url)
                    .header(ACCEPT, "text/plain")
                    .timeout(LOG_TIMEOUT)
                    .send()
            })
            .await?;
        if matches!(response.status(), StatusCode::NOT_FOUND | StatusCode::GONE) {
            return Err(PluginError::PipelineNotFound(format!(
                "Log {log_id} of build {build_id} is no longer available"
            )));
        }
        signed_in(response).await
    }

    pub async fn queue_build(&self, project: &str, body: &Value) -> PluginResult<Build> {
        let response = self
            .send_json(Method::POST, project, "_apis/build/builds", body)
            .await?;
        parse(response).await
    }

    pub async fn cancel_build(&self, project: &str, build_id: i64) -> PluginResult<()> {
        let path = format!("_apis/build/builds/{build_id}");
        self.send_json(
            Method::PATCH,
            project,
            &path,
            &json!({ "status": "cancelling" }),
        )
        .await
        .map(drop)
    }

    fn url(
        &self, project: Option<&str>, path: &str, query: &[(&str, String)],
    ) -> PluginResult<Url> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .map_err(|()| PluginError::InvalidConfig("Invalid Azure DevOps URL".to_string()))?
            .pop_if_empty()
            .extend(project)
            .extend(path.split('/'));
        url.query_pairs_mut()
            .append_pair("api-version", API_VERSION)
            .extend_pairs(query);
        Ok(url)
    }

    fn authorized(&self, method: Method, url: &Url) -> reqwest::RequestBuilder {
        self.http_client
            .request(method, url.clone())
            .basic_auth("", Some(&self.token))
    }

    fn request(&self, method: Method, url: &Url) -> reqwest::RequestBuilder {
        self.authorized(method, url)
            .header(ACCEPT, "application/json")
    }

    async fn send_json(
        &self, method: Method, project: &str, path: &str, body: &Value,
    ) -> PluginResult<reqwest::Response> {
        let url = self.url(Some(project), path, &[])?;
        let response = self
            .retry_policy
            .retry_unsent(&url, || {
                self.request(method.clone(), &url).json(body).send()
            })
            .await?;
        signed_in(response).await
    }

    async fn get_json<T: DeserializeOwned>(
        &self, project: Option<&str>, path: &str, query: &[(&str, String)],
    ) -> PluginResult<(T, Option<String>)> {
        let url = self.url(project, path, query)?;
        self.retry_policy
            .retry(|| async {
                let response = self
                    .request(Method::GET, &url)
                    .send()
                    .await
                    .map_err(|e| map_send_error(e, &url))?;
                let response = signed_in(response).await?;
                let continuation = response
                    .headers()
                    .get(CONTINUATION_HEADER)
                    .and_then(|value| value.to_str().ok())
                    .filter(|token| !token.is_empty())
                    .map(str::to_string);
                Ok((parse(response).await?, continuation))
            })
            .await
    }

    async fn list<T: DeserializeOwned>(
        &self, project: Option<&str>, path: &str, query: &[(&str, String)], page_size: usize,
        max: usize,
    ) -> PluginResult<Vec<T>> {
        let mut items = Vec::new();
        let mut continuation: Option<String> = None;
        for _ in 0..MAX_PAGES {
            if items.len() >= max {
                break;
            }
            let mut page_query = query.to_vec();
            page_query.push(("$top", page_size.min(max - items.len()).to_string()));
            if let Some(token) = continuation.take() {
                page_query.push(("continuationToken", token));
            }
            let (page, next): (ListResponse<T>, _) =
                self.get_json(project, path, &page_query).await?;
            items.extend(page.value);
            continuation = next;
            if continuation.is_none() {
                break;
            }
        }
        if continuation.is_some() && items.len() < max {
            return Err(PluginError::ApiError(format!(
                "Azure DevOps list {path} still has more items after {MAX_PAGES} pages"
            )));
        }
        items.truncate(max);
        Ok(items)
    }
}

async fn signed_in(response: reqwest::Response) -> PluginResult<reqwest::Response> {
    let response = check_response(response).await?;
    let status = response.status();
    let html = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.get(..9))
        .is_some_and(|media_type| media_type.eq_ignore_ascii_case("text/html"));
    if status == StatusCode::NON_AUTHORITATIVE_INFORMATION || status.is_redirection() || html {
        return Err(PluginError::AuthenticationFailed(format!(
            "{} from {}: Azure DevOps answered with a sign-in page instead of data. Check that the personal access token is valid and has not expired",
            status.as_u16(),
            redact_url(response.url())
        )));
    }
    Ok(response)
}

async fn parse<T: DeserializeOwned>(response: reqwest::Response) -> PluginResult<T> {
    let url = redact_url(response.url());
    response.json().await.map_err(|e| {
        PluginError::ApiError(format!(
            "Failed to parse Azure DevOps API response from {url}: {}",
            e.without_url()
        ))
    })
}
