use std::sync::Arc;
use std::time::Duration;

use pipedash_plugin_api::http::{
    api_url,
    bearer_request,
    decode_json,
    get_json,
    send_checked,
};
use pipedash_plugin_api::{
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
        decode_json(response, "Woodpecker API response")
            .await
            .map(Some)
    }

    pub async fn cancel_pipeline(&self, repo_id: i64, number: i64) -> PluginResult<()> {
        self.post(&format!("/repos/{repo_id}/pipelines/{number}/cancel"), None)
            .await
            .map(drop)
    }

    fn url(&self, path: &str, query: &[(&str, String)]) -> PluginResult<Url> {
        api_url(&format!("{}/api", self.base_url), path, query, "Woodpecker")
    }

    async fn get<T: DeserializeOwned>(
        &self, path: &str, query: &[(&str, String)],
    ) -> PluginResult<T> {
        self.get_url(&self.url(path, query)?, None).await
    }

    async fn get_url<T: DeserializeOwned>(
        &self, url: &Url, timeout: Option<Duration>,
    ) -> PluginResult<T> {
        get_json(&self.retry_policy, url, "Woodpecker API response", || {
            let request = bearer_request(&self.http_client, Method::GET, url, &self.token);
            match timeout {
                Some(timeout) => request.timeout(timeout),
                None => request,
            }
        })
        .await
    }

    async fn post(
        &self, path: &str, body: Option<&serde_json::Value>,
    ) -> PluginResult<reqwest::Response> {
        let url = self.url(path, &[])?;
        send_checked(&self.retry_policy, &url, || {
            let request = bearer_request(&self.http_client, Method::POST, &url, &self.token);
            match body {
                Some(body) => request.json(body),
                None => request,
            }
        })
        .await
    }
}
