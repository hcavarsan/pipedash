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
    Pipeline,
    PipelineVariable,
    Project,
    TriggerPipelineRequest,
    User,
};

pub struct GitLabClient {
    http_client: reqwest::Client,
    api_url: String,
    retry_policy: RetryPolicy,
    user_cache: OnceLock<User>,
}

impl GitLabClient {
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

    pub async fn list_groups(&self) -> PluginResult<Vec<pipedash_plugin_api::Organization>> {
        let user = self.get_user().await?;
        let mut orgs = vec![pipedash_plugin_api::Organization {
            id: user.id.to_string(),
            name: user.username.clone(),
            description: Some(user.name.clone()),
        }];

        let groups_result = self
            .retry_policy
            .retry(|| async {
                let url = format!("{}/groups?per_page=100", self.api_url);
                let response = self.http_client.get(&url).send().await.map_err(|e| {
                    PluginError::NetworkError(format!("Failed to list groups: {}", e))
                })?;

                let groups: Vec<serde_json::Value> = self.handle_response(response).await?;

                Ok(groups
                    .into_iter()
                    .map(|g| pipedash_plugin_api::Organization {
                        id: g["id"].as_i64().unwrap_or(0).to_string(),
                        name: g["name"].as_str().unwrap_or("").to_string(),
                        description: g["description"].as_str().map(|s| s.to_string()),
                    })
                    .collect::<Vec<_>>())
            })
            .await;

        if let Ok(mut groups) = groups_result {
            orgs.append(&mut groups);
        }

        Ok(orgs)
    }

    pub async fn list_projects(
        &self, params: &PaginationParams,
    ) -> PluginResult<PaginatedResponse<Project>> {
        self.retry_policy
            .retry(|| async {
                let url = format!(
                    "{}/projects?membership=true&per_page={}&page={}",
                    self.api_url, params.page_size, params.page
                );
                let response = self.http_client.get(&url).send().await.map_err(|e| {
                    PluginError::NetworkError(format!("Failed to list projects: {}", e))
                })?;

                let total_count = response
                    .headers()
                    .get("x-total")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<usize>().ok());

                let items: Vec<Project> = self.handle_response(response).await?;
                let count = total_count.unwrap_or(items.len());

                Ok(PaginatedResponse::new(
                    items,
                    params.page,
                    params.page_size,
                    count,
                ))
            })
            .await
    }

    pub async fn list_projects_filtered(
        &self, group_id: Option<String>, search: Option<String>, params: &PaginationParams,
    ) -> PluginResult<PaginatedResponse<Project>> {
        let user = self.get_user().await?;
        let is_personal_account = group_id
            .as_ref()
            .map(|gid| gid == &user.id.to_string())
            .unwrap_or(false);

        self.retry_policy
            .retry(|| async {
                let mut url = if let Some(gid) = &group_id {
                    if is_personal_account {
                        format!(
                            "{}/users/{}/projects?per_page={}&page={}",
                            self.api_url, user.id, params.page_size, params.page
                        )
                    } else {
                        format!(
                            "{}/groups/{}/projects?per_page={}&page={}",
                            self.api_url, gid, params.page_size, params.page
                        )
                    }
                } else {
                    format!(
                        "{}/projects?membership=true&per_page={}&page={}",
                        self.api_url, params.page_size, params.page
                    )
                };

                if let Some(search_term) = &search {
                    let encoded = search_term.replace(" ", "%20").replace("&", "%26");
                    url.push_str(&format!("&search={}", encoded));
                }

                let response = self.http_client.get(&url).send().await.map_err(|e| {
                    PluginError::NetworkError(format!("Failed to list projects: {}", e))
                })?;

                let total_count = response
                    .headers()
                    .get("x-total")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<usize>().ok());

                let items: Vec<Project> = self.handle_response(response).await?;
                let count = total_count.unwrap_or(items.len());

                Ok(PaginatedResponse::new(
                    items,
                    params.page,
                    params.page_size,
                    count,
                ))
            })
            .await
    }

    pub async fn get_project(&self, project_id: i64) -> PluginResult<Project> {
        self.retry_policy
            .retry(|| async {
                let url = format!("{}/projects/{}", self.api_url, project_id);
                let response = self.http_client.get(&url).send().await.map_err(|e| {
                    PluginError::NetworkError(format!("Failed to get project: {}", e))
                })?;

                self.handle_response(response).await
            })
            .await
    }

    pub async fn get_project_pipelines(
        &self, project_id: i64, per_page: usize,
    ) -> PluginResult<Vec<Pipeline>> {
        self.retry_policy
            .retry(|| async {
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
        self.retry_policy
            .retry(|| async {
                let url = format!(
                    "{}/projects/{}/pipelines/{}",
                    self.api_url, project_id, pipeline_id
                );
                let response = self.http_client.get(&url).send().await.map_err(|e| {
                    PluginError::NetworkError(format!("Failed to get pipeline: {}", e))
                })?;

                self.handle_response(response).await
            })
            .await
    }

    pub async fn trigger_pipeline(
        &self, project_id: i64, ref_name: String, variables: Option<Vec<PipelineVariable>>,
    ) -> PluginResult<Pipeline> {
        self.retry_policy
            .retry(|| async {
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
        self.retry_policy
            .retry(|| async {
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
        let url = response.url().clone();

        // Log response details for debugging
        let content_length = response.content_length();
        eprintln!(
            "[GITLAB DEBUG] Response from {}: status={}, content_length={:?}",
            url, status, content_length
        );

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
                "GitLab API error ({}) for {}: {}",
                status, url, error_text
            )));
        }

        // Attempt to deserialize with better error context
        response.json::<T>().await.map_err(|e| {
            eprintln!("[GITLAB ERROR] Failed to deserialize response from {}: {}", url, e);
            PluginError::ApiError(format!(
                "Failed to parse GitLab API response from {}: {}. This may indicate a network issue or incompatible response format.",
                url, e
            ))
        })
    }
}
