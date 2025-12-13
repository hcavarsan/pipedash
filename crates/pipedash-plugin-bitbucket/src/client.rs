use std::sync::OnceLock;

use pipedash_plugin_api::{
    PaginatedResponse,
    PaginationParams,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use reqwest::StatusCode;

use crate::types::{
    PaginatedResponse as BitbucketPaginatedResponse,
    Pipeline,
    PipelineStep,
    Repository,
    TriggerPipelineRequest,
    User,
    Workspace,
};

pub struct BitbucketClient {
    http_client: reqwest::Client,
    api_url: String,
    retry_policy: RetryPolicy,
    user_cache: OnceLock<User>,
}

impl BitbucketClient {
    pub fn new(http_client: reqwest::Client, api_url: String) -> Self {
        Self {
            http_client,
            api_url: api_url.trim_end_matches('/').to_string(),
            retry_policy: RetryPolicy::default(),
            user_cache: OnceLock::new(),
        }
    }

    pub async fn get_user(&self) -> PluginResult<User> {
        if let Some(user) = self.user_cache.get() {
            return Ok(user.clone());
        }

        let user: User =
            self.retry_policy
                .retry(|| async {
                    let url = format!("{}/user", self.api_url);
                    let response = self.http_client.get(&url).send().await.map_err(|e| {
                        PluginError::NetworkError(format!("Failed to get user: {}", e))
                    })?;

                    self.handle_response(response).await
                })
                .await?;

        let _ = self.user_cache.set(user.clone());
        Ok(user)
    }

    pub async fn list_workspaces(&self) -> PluginResult<Vec<Workspace>> {
        let mut all_workspaces = Vec::new();
        let mut next_url = Some(format!("{}/workspaces?pagelen=100", self.api_url));
        const MAX_PAGES: usize = 10;
        let mut page_count = 0;

        while let Some(url) = next_url.take() {
            if page_count >= MAX_PAGES {
                break;
            }

            let paginated: BitbucketPaginatedResponse<Workspace> = self
                .retry_policy
                .retry(|| async {
                    let response = self.http_client.get(&url).send().await.map_err(|e| {
                        PluginError::NetworkError(format!("Failed to list workspaces: {}", e))
                    })?;
                    self.handle_response(response).await
                })
                .await?;

            all_workspaces.extend(paginated.values);
            next_url = paginated.next;
            page_count += 1;
        }

        Ok(all_workspaces)
    }

    pub async fn list_repositories(
        &self, workspace: &str, params: &PaginationParams,
    ) -> PluginResult<PaginatedResponse<Repository>> {
        self.retry_policy
            .retry(|| async {
                let url = format!(
                    "{}/repositories/{}?pagelen={}&page={}",
                    self.api_url, workspace, params.page_size, params.page
                );
                let response = self.http_client.get(&url).send().await.map_err(|e| {
                    PluginError::NetworkError(format!("Failed to list repositories: {}", e))
                })?;

                let paginated: BitbucketPaginatedResponse<Repository> =
                    self.handle_response(response).await?;

                let total_count = paginated.size.unwrap_or(paginated.values.len());
                Ok(PaginatedResponse::new(
                    paginated.values,
                    params.page,
                    params.page_size,
                    total_count,
                ))
            })
            .await
    }

    pub async fn list_all_repositories(
        &self, params: &PaginationParams,
    ) -> PluginResult<PaginatedResponse<Repository>> {
        self.retry_policy
            .retry(|| async {
                let url = format!(
                    "{}/repositories?role=member&pagelen={}&page={}",
                    self.api_url, params.page_size, params.page
                );
                let response = self.http_client.get(&url).send().await.map_err(|e| {
                    PluginError::NetworkError(format!("Failed to list repositories: {}", e))
                })?;

                let paginated: BitbucketPaginatedResponse<Repository> =
                    self.handle_response(response).await?;

                let total_count = paginated.size.unwrap_or(paginated.values.len());
                Ok(PaginatedResponse::new(
                    paginated.values,
                    params.page,
                    params.page_size,
                    total_count,
                ))
            })
            .await
    }

    /// Bitbucket API max pagelen is 100
    pub async fn list_pipelines(
        &self, workspace: &str, repo_slug: &str, limit: usize,
    ) -> PluginResult<Vec<Pipeline>> {
        let pagelen = limit.min(100);
        self.retry_policy
            .retry(|| async {
                let url = format!(
                    "{}/repositories/{}/{}/pipelines?pagelen={}&sort=-created_on",
                    self.api_url, workspace, repo_slug, pagelen
                );
                let response = self.http_client.get(&url).send().await.map_err(|e| {
                    PluginError::NetworkError(format!("Failed to list pipelines: {}", e))
                })?;

                let paginated: BitbucketPaginatedResponse<Pipeline> =
                    self.handle_response(response).await?;
                Ok(paginated.values)
            })
            .await
    }

    pub async fn list_steps(
        &self, workspace: &str, repo_slug: &str, pipeline_uuid: &str,
    ) -> PluginResult<Vec<PipelineStep>> {
        // Bitbucket UUIDs have curly braces that need URL encoding
        let uuid_with_braces = if pipeline_uuid.starts_with('{') {
            pipeline_uuid.to_string()
        } else {
            format!("{{{}}}", pipeline_uuid)
        };
        let encoded_uuid = urlencoding::encode(&uuid_with_braces);

        self.retry_policy
            .retry(|| async {
                let url = format!(
                    "{}/repositories/{}/{}/pipelines/{}/steps",
                    self.api_url, workspace, repo_slug, encoded_uuid
                );
                let response = self.http_client.get(&url).send().await.map_err(|e| {
                    PluginError::NetworkError(format!("Failed to list pipeline steps: {}", e))
                })?;

                let paginated: BitbucketPaginatedResponse<PipelineStep> =
                    self.handle_response(response).await?;
                Ok(paginated.values)
            })
            .await
    }

    pub async fn trigger_pipeline(
        &self, workspace: &str, repo_slug: &str, request: TriggerPipelineRequest,
    ) -> PluginResult<Pipeline> {
        self.retry_policy
            .retry(|| async {
                let url = format!(
                    "{}/repositories/{}/{}/pipelines/",
                    self.api_url, workspace, repo_slug
                );
                let response = self
                    .http_client
                    .post(&url)
                    .json(&request)
                    .send()
                    .await
                    .map_err(|e| {
                        PluginError::NetworkError(format!("Failed to trigger pipeline: {}", e))
                    })?;

                self.handle_response(response).await
            })
            .await
    }

    pub async fn stop_pipeline(
        &self, workspace: &str, repo_slug: &str, pipeline_uuid: &str,
    ) -> PluginResult<()> {
        // Bitbucket UUIDs have curly braces that need URL encoding
        let uuid_with_braces = if pipeline_uuid.starts_with('{') {
            pipeline_uuid.to_string()
        } else {
            format!("{{{}}}", pipeline_uuid)
        };
        let encoded_uuid = urlencoding::encode(&uuid_with_braces);

        self.retry_policy
            .retry(|| async {
                let url = format!(
                    "{}/repositories/{}/{}/pipelines/{}/stopPipeline",
                    self.api_url, workspace, repo_slug, encoded_uuid
                );
                let response = self.http_client.post(&url).send().await.map_err(|e| {
                    PluginError::NetworkError(format!("Failed to stop pipeline: {}", e))
                })?;

                let status = response.status();
                if status.is_success() {
                    Ok(())
                } else {
                    let error_text = response.text().await.unwrap_or_default();
                    Err(PluginError::ApiError(format!(
                        "Failed to stop pipeline ({}): {}",
                        status, error_text
                    )))
                }
            })
            .await
    }

    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self, response: reqwest::Response,
    ) -> PluginResult<T> {
        let status = response.status();
        let url = response.url().clone();

        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(PluginError::AuthenticationFailed(format!(
                "Authentication failed for {}: {}",
                url, error_text
            )));
        }

        if status == StatusCode::NOT_FOUND {
            return Err(PluginError::PipelineNotFound(format!(
                "Resource not found: {}",
                url
            )));
        }

        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(PluginError::ApiError(format!(
                "Bitbucket API error ({}) for {}: {}",
                status, url, error_text
            )));
        }

        response.json::<T>().await.map_err(|e| {
            PluginError::ApiError(format!(
                "Failed to parse Bitbucket API response from {}: {}",
                url, e
            ))
        })
    }
}
