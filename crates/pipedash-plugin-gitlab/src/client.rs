use std::time::Duration;

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};
use reqwest::StatusCode;

use crate::types::{
    Pipeline,
    PipelineVariable,
    Project,
    TriggerPipelineRequest,
    User,
};

pub struct GitLabClient {
    http_client: reqwest::Client,
    api_url: String,
}

impl GitLabClient {
    pub fn new(http_client: reqwest::Client, api_url: String) -> Self {
        Self {
            http_client,
            api_url: api_url.trim_end_matches('/').to_string(),
        }
    }

    pub async fn get_user(&self) -> PluginResult<User> {
        self.retry_request(|| async {
            let url = format!("{}/user", self.api_url);
            let response = self
                .http_client
                .get(&url)
                .send()
                .await
                .map_err(|e| PluginError::NetworkError(format!("Failed to get user: {}", e)))?;

            self.handle_response(response).await
        })
        .await
    }

    pub async fn list_projects(&self, page: u32) -> PluginResult<Vec<Project>> {
        self.retry_request(|| async {
            let url = format!(
                "{}/projects?membership=true&per_page=100&page={}",
                self.api_url, page
            );
            let response = self.http_client.get(&url).send().await.map_err(|e| {
                PluginError::NetworkError(format!("Failed to list projects: {}", e))
            })?;

            self.handle_response(response).await
        })
        .await
    }

    pub async fn get_project_pipelines(
        &self, project_id: i64, per_page: usize,
    ) -> PluginResult<Vec<Pipeline>> {
        self.retry_request(|| async {
            let url = format!(
                "{}/projects/{}/pipelines?per_page={}&order_by=updated_at",
                self.api_url, project_id, per_page
            );
            let response = self.http_client.get(&url).send().await.map_err(|e| {
                PluginError::NetworkError(format!("Failed to get project pipelines: {}", e))
            })?;

            self.handle_response(response).await
        })
        .await
    }

    pub async fn get_pipeline(&self, project_id: i64, pipeline_id: i64) -> PluginResult<Pipeline> {
        self.retry_request(|| async {
            let url = format!(
                "{}/projects/{}/pipelines/{}",
                self.api_url, project_id, pipeline_id
            );
            let response =
                self.http_client.get(&url).send().await.map_err(|e| {
                    PluginError::NetworkError(format!("Failed to get pipeline: {}", e))
                })?;

            self.handle_response(response).await
        })
        .await
    }

    pub async fn trigger_pipeline(
        &self, project_id: i64, ref_name: String, variables: Option<Vec<PipelineVariable>>,
    ) -> PluginResult<Pipeline> {
        self.retry_request(|| async {
            let url = format!("{}/projects/{}/pipeline", self.api_url, project_id);
            let request_body = TriggerPipelineRequest {
                ref_name: ref_name.clone(),
                variables: variables.clone(),
            };

            let response = self
                .http_client
                .post(&url)
                .json(&request_body)
                .send()
                .await
                .map_err(|e| {
                    PluginError::NetworkError(format!("Failed to trigger pipeline: {}", e))
                })?;

            self.handle_response(response).await
        })
        .await
    }

    pub async fn cancel_pipeline(
        &self, project_id: i64, pipeline_id: i64,
    ) -> PluginResult<Pipeline> {
        self.retry_request(|| async {
            let url = format!(
                "{}/projects/{}/pipelines/{}/cancel",
                self.api_url, project_id, pipeline_id
            );
            let response = self.http_client.post(&url).send().await.map_err(|e| {
                PluginError::NetworkError(format!("Failed to cancel pipeline: {}", e))
            })?;

            self.handle_response(response).await
        })
        .await
    }

    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self, response: reqwest::Response,
    ) -> PluginResult<T> {
        let status = response.status();

        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(PluginError::AuthenticationFailed(format!(
                "Authentication failed: {}",
                error_text
            )));
        }

        if status == StatusCode::NOT_FOUND {
            return Err(PluginError::PipelineNotFound(
                "Resource not found".to_string(),
            ));
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(PluginError::ApiError(format!(
                "GitLab API error ({}): {}",
                status, error_text
            )));
        }

        response.json::<T>().await.map_err(|e| {
            PluginError::ApiError(format!("Failed to parse GitLab API response: {}", e))
        })
    }

    async fn retry_request<F, Fut, T>(&self, operation: F) -> PluginResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = PluginResult<T>>,
    {
        let max_retries = 3;
        let mut delay = Duration::from_millis(100);

        for attempt in 0..max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) if attempt < max_retries - 1 => match &e {
                    PluginError::NetworkError(_) | PluginError::ApiError(_) => {
                        tokio::time::sleep(delay).await;
                        delay *= 2;
                        continue;
                    }
                    _ => return Err(e),
                },
                Err(e) => return Err(e),
            }
        }

        unreachable!()
    }
}
