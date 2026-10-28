use std::sync::Arc;

use pipedash_plugin_api::{
    check_response,
    map_send_error,
    redact_url,
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

use crate::types::{
    Build,
    Definition,
    DefinitionDetail,
    ListResponse,
    Project,
};

const API_VERSION: &str = "7.0";
const CONTINUATION_HEADER: &str = "x-ms-continuationtoken";
const PAGE_SIZE: usize = 100;
const MAX_PAGES: usize = 100;

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

    fn request(&self, method: Method, url: &Url) -> reqwest::RequestBuilder {
        self.http_client
            .request(method, url.clone())
            .basic_auth("", Some(&self.token))
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
