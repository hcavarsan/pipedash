use std::sync::Arc;

use pipedash_plugin_api::http::{
    api_url,
    bearer_request,
    decode_json,
    get_json,
    send,
    send_checked,
};
use pipedash_plugin_api::{
    check_response,
    map_send_error,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use reqwest::{
    Method,
    Url,
};
use serde::de::DeserializeOwned;

use crate::types::{
    Application,
    DeployResponse,
    Deployment,
    DeploymentDetail,
    DeploymentPage,
    Project,
    ProjectDetail,
};

const HISTORY_PAGE_SIZE: usize = 20;

pub(crate) struct CoolifyClient {
    http_client: Arc<reqwest::Client>,
    server_url: String,
    token: String,
    retry_policy: RetryPolicy,
}

impl CoolifyClient {
    pub fn new(http_client: Arc<reqwest::Client>, server_url: String, token: String) -> Self {
        Self {
            http_client,
            server_url,
            token,
            retry_policy: RetryPolicy::default(),
        }
    }

    pub async fn version(&self) -> PluginResult<String> {
        let url = self.url("/version", &[])?;
        self.retry_policy
            .retry(|| async {
                let response = send(
                    &url,
                    bearer_request(&self.http_client, Method::GET, &url, &self.token),
                )
                .await?;
                let text = check_response(response)
                    .await?
                    .text()
                    .await
                    .map_err(|e| map_send_error(e, &url))?;
                Ok(text.trim().to_string())
            })
            .await
    }

    pub async fn applications(&self) -> PluginResult<Vec<Application>> {
        self.get("/applications", &[]).await
    }

    pub async fn projects(&self) -> PluginResult<Vec<Project>> {
        self.get("/projects", &[]).await
    }

    pub async fn project(&self, uuid: &str) -> PluginResult<ProjectDetail> {
        self.get(&format!("/projects/{uuid}"), &[]).await
    }

    pub async fn deployments(
        &self, app_uuid: &str, skip: usize, take: usize,
    ) -> PluginResult<Vec<Deployment>> {
        let page: DeploymentPage = self
            .get(
                &format!("/deployments/applications/{app_uuid}"),
                &[("skip", skip.to_string()), ("take", take.to_string())],
            )
            .await?;
        Ok(page.deployments)
    }

    pub async fn recent_deployments(
        &self, app_uuid: &str, limit: usize,
    ) -> PluginResult<Vec<Deployment>> {
        let mut out = Vec::new();
        while out.len() < limit {
            let take = (limit - out.len()).min(HISTORY_PAGE_SIZE);
            let page = self.deployments(app_uuid, out.len(), take).await?;
            let last_page = page.len() < take;
            out.extend(page);
            if last_page {
                break;
            }
        }
        Ok(out)
    }

    pub async fn find_deployment(&self, app_uuid: &str, id: i64) -> PluginResult<Deployment> {
        let mut skip = 0;
        loop {
            let page = self.deployments(app_uuid, skip, HISTORY_PAGE_SIZE).await?;
            let last_page = page.len() < HISTORY_PAGE_SIZE || page.iter().any(|item| item.id < id);
            if let Some(found) = page.into_iter().find(|item| item.id == id) {
                return Ok(found);
            }
            if last_page {
                break;
            }
            skip += HISTORY_PAGE_SIZE;
        }
        Err(PluginError::PipelineNotFound(format!(
            "Deployment {id} not found for application {app_uuid}"
        )))
    }

    pub async fn deployment(&self, deployment_uuid: &str) -> PluginResult<DeploymentDetail> {
        self.get(&format!("/deployments/{deployment_uuid}"), &[])
            .await
    }

    pub async fn deploy(&self, app_uuid: &str) -> PluginResult<DeployResponse> {
        let response = self
            .post("/deploy", &[("uuid", app_uuid.to_string())])
            .await?;
        decode_json(response, "Coolify API response").await
    }

    pub async fn cancel(&self, deployment_uuid: &str) -> PluginResult<()> {
        self.post(&format!("/deployments/{deployment_uuid}/cancel"), &[])
            .await
            .map(drop)
    }

    fn url(&self, path: &str, query: &[(&str, String)]) -> PluginResult<Url> {
        api_url(
            &format!("{}/api/v1", self.server_url),
            path,
            query,
            "Coolify",
        )
    }

    async fn get<T: DeserializeOwned>(
        &self, path: &str, query: &[(&str, String)],
    ) -> PluginResult<T> {
        let url = self.url(path, query)?;
        get_json(&self.retry_policy, &url, "Coolify API response", || {
            bearer_request(&self.http_client, Method::GET, &url, &self.token)
        })
        .await
    }

    async fn post(&self, path: &str, query: &[(&str, String)]) -> PluginResult<reqwest::Response> {
        let url = self.url(path, query)?;
        send_checked(&self.retry_policy, &url, || {
            bearer_request(&self.http_client, Method::POST, &url, &self.token)
        })
        .await
    }
}
