use std::sync::Arc;
use std::time::Duration;

use pipedash_plugin_api::{
    check_response,
    map_send_error,
    redact_url,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use reqwest::{
    Method,
    StatusCode,
    Url,
};
use serde::de::DeserializeOwned;

use crate::types::{
    LogEntry,
    Pipeline,
    Repo,
};

const PIPELINES_PAGE_SIZE: usize = 50;
const LOG_TIMEOUT: Duration = Duration::from_secs(900);

pub(crate) struct WoodpeckerClient {
    http_client: Arc<reqwest::Client>,
    base_url: String,
    token: String,
    retry_policy: RetryPolicy,
}

impl WoodpeckerClient {
    pub fn new(http_client: Arc<reqwest::Client>, base_url: String, token: String) -> Self {
        Self {
            http_client,
            base_url,
            token,
            retry_policy: RetryPolicy::default(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn ping(&self) -> PluginResult<()> {
        self.get::<serde::de::IgnoredAny>("/user", &[])
            .await
            .map(drop)
    }

    pub async fn list_repos(&self) -> PluginResult<Vec<Repo>> {
        self.get("/user/repos", &[]).await
    }

    pub async fn lookup_repo(&self, full_name: &str) -> PluginResult<Repo> {
        self.get(&format!("/repos/lookup/{full_name}"), &[]).await
    }

    pub async fn get_repo(&self, repo_id: i64) -> PluginResult<Repo> {
        self.get(&format!("/repos/{repo_id}"), &[]).await
    }

    pub async fn list_pipelines(&self, repo_id: i64, limit: usize) -> PluginResult<Vec<Pipeline>> {
        let per_page = limit.min(PIPELINES_PAGE_SIZE);
        let path = format!("/repos/{repo_id}/pipelines");
        let mut pipelines = Vec::with_capacity(limit);
        for page in 1.. {
            if pipelines.len() >= limit {
                break;
            }
            let batch: Vec<Pipeline> = self
                .get(
                    &path,
                    &[
                        ("page", page.to_string()),
                        ("perPage", per_page.to_string()),
                    ],
                )
                .await?;
            if batch.is_empty() {
                break;
            }
            pipelines.extend(batch);
        }
        pipelines.truncate(limit);
        Ok(pipelines)
    }

    pub async fn get_pipeline(&self, repo_id: i64, number: i64) -> PluginResult<Pipeline> {
        self.get(&format!("/repos/{repo_id}/pipelines/{number}"), &[])
            .await
    }

    pub async fn step_logs(
        &self, repo_id: i64, number: i64, step_id: &str,
    ) -> PluginResult<Vec<LogEntry>> {
        let url = self.url(&format!("/repos/{repo_id}/logs/{number}/{step_id}"), &[])?;
        self.get_url(&url, Some(LOG_TIMEOUT))
            .await
            .map_err(|e| match e {
                PluginError::PipelineNotFound(_) => PluginError::PipelineNotFound(format!(
                    "Logs for step {step_id} are no longer available"
                )),
                e => e,
            })
    }

    pub async fn create_pipeline(
        &self, repo_id: i64, branch: &str,
    ) -> PluginResult<Option<Pipeline>> {
        let response = self
            .post(
                &format!("/repos/{repo_id}/pipelines"),
                Some(&serde_json::json!({ "branch": branch })),
            )
            .await?;
        if response.status() == StatusCode::NO_CONTENT {
            return Ok(None);
        }
        parse(response).await.map(Some)
    }

    pub async fn cancel_pipeline(&self, repo_id: i64, number: i64) -> PluginResult<()> {
        self.post(&format!("/repos/{repo_id}/pipelines/{number}/cancel"), None)
            .await
            .map(drop)
    }

    fn url(&self, path: &str, query: &[(&str, String)]) -> PluginResult<Url> {
        let mut url = Url::parse(&format!("{}/api{path}", self.base_url))
            .map_err(|e| PluginError::InvalidConfig(format!("Invalid Woodpecker URL: {e}")))?;
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        Ok(url)
    }

    fn request(&self, method: Method, url: &Url) -> reqwest::RequestBuilder {
        self.http_client
            .request(method, url.clone())
            .bearer_auth(&self.token)
    }

    async fn get<T: DeserializeOwned>(
        &self, path: &str, query: &[(&str, String)],
    ) -> PluginResult<T> {
        self.get_url(&self.url(path, query)?, None).await
    }

    async fn get_url<T: DeserializeOwned>(
        &self, url: &Url, timeout: Option<Duration>,
    ) -> PluginResult<T> {
        self.retry_policy
            .retry(|| async {
                let mut request = self.request(Method::GET, url);
                if let Some(timeout) = timeout {
                    request = request.timeout(timeout);
                }
                let response = request.send().await.map_err(|e| map_send_error(e, url))?;
                parse(check_response(response).await?).await
            })
            .await
    }

    async fn post(
        &self, path: &str, body: Option<&serde_json::Value>,
    ) -> PluginResult<reqwest::Response> {
        let url = self.url(path, &[])?;
        let response = self
            .retry_policy
            .retry_unsent(&url, || {
                let request = self.request(Method::POST, &url);
                match body {
                    Some(body) => request.json(body),
                    None => request,
                }
                .send()
            })
            .await?;
        check_response(response).await
    }
}

async fn parse<T: DeserializeOwned>(response: reqwest::Response) -> PluginResult<T> {
    let url = redact_url(response.url());
    response.json().await.map_err(|e| {
        PluginError::ApiError(format!(
            "Failed to parse Woodpecker API response from {url}: {}",
            e.without_url()
        ))
    })
}
