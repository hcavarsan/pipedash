use std::sync::OnceLock;

use pipedash_plugin_api::{
    check_response,
    map_send_error,
    PaginatedResponse,
    PaginationParams,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use reqwest::Url;

use crate::types::{
    Pipeline,
    PipelineVariable,
    Project,
    TriggerPipelineRequest,
    User,
};

pub struct GitLabClient {
    http_client: std::sync::Arc<reqwest::Client>,
    api_url: String,
    token: String,
    retry_policy: RetryPolicy,
    user_cache: OnceLock<User>,
}

impl GitLabClient {
    pub fn new(
        http_client: std::sync::Arc<reqwest::Client>, api_url: String, token: String,
    ) -> Self {
        Self {
            http_client,
            api_url: api_url.trim_end_matches('/').to_string(),
            token,
            retry_policy: RetryPolicy::default(),
            user_cache: OnceLock::new(),
        }
    }

    pub async fn get_user(&self) -> PluginResult<User> {
        if let Some(user) = self.user_cache.get() {
            return Ok(user.clone());
        }

        let user: User = self
            .retry_policy
            .retry(|| async {
                let response = self.get(&format!("{}/user", self.api_url)).await?;
                Self::json(response).await
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

        match self.list_all_groups().await {
            Ok(mut groups) => orgs.append(&mut groups),
            Err(e) => tracing::warn!(error = %e, "Failed to list GitLab groups"),
        }

        Ok(orgs)
    }

    async fn list_all_groups(&self) -> PluginResult<Vec<pipedash_plugin_api::Organization>> {
        let mut groups = Vec::new();
        let mut page = Some(1);

        while let Some(current) = page {
            let (mut items, next_page) = self
                .retry_policy
                .retry(|| async {
                    let response = self
                        .get(&format!(
                            "{}/groups?per_page=100&page={current}",
                            self.api_url
                        ))
                        .await?;
                    let next_page = Self::next_page(&response);
                    let items: Vec<serde_json::Value> = Self::json(response).await?;

                    Ok((
                        items
                            .into_iter()
                            .map(|g| pipedash_plugin_api::Organization {
                                id: g["id"].as_i64().unwrap_or(0).to_string(),
                                name: g["full_path"]
                                    .as_str()
                                    .or_else(|| g["name"].as_str())
                                    .unwrap_or("")
                                    .to_string(),
                                description: g["description"].as_str().map(|s| s.to_string()),
                            })
                            .collect::<Vec<_>>(),
                        next_page,
                    ))
                })
                .await?;
            groups.append(&mut items);
            page = next_page.filter(|next| *next > current);
        }

        Ok(groups)
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
                self.project_page(&url, params).await
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
                            "{}/groups/{}/projects?include_subgroups=true&per_page={}&page={}",
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

                self.project_page(&url, params).await
            })
            .await
    }

    pub async fn get_project(&self, project_id: i64) -> PluginResult<Project> {
        self.retry_policy
            .retry(|| async {
                let response = self
                    .get(&format!("{}/projects/{}", self.api_url, project_id))
                    .await?;
                Self::json(response).await
            })
            .await
    }

    pub async fn get_project_pipelines(
        &self, project_id: i64, limit: usize,
    ) -> PluginResult<Vec<Pipeline>> {
        let per_page = limit.clamp(1, 100);
        let mut pipelines = Vec::new();
        let mut page = Some(1);

        while let Some(current) = page.filter(|_| pipelines.len() < limit) {
            let (mut items, next_page) = self
                .retry_policy
                .retry(|| async {
                    let response = self
                        .get(&format!(
                            "{}/projects/{}/pipelines?per_page={}&page={}&order_by=updated_at",
                            self.api_url, project_id, per_page, current
                        ))
                        .await?;
                    let next_page = Self::next_page(&response);
                    let items: Vec<Pipeline> = Self::json(response).await?;

                    Ok((items, next_page))
                })
                .await?;
            pipelines.append(&mut items);
            page = next_page.filter(|next| *next > current);
        }

        pipelines.truncate(limit);
        Ok(pipelines)
    }

    pub async fn has_ci_config(&self, project: &Project) -> PluginResult<bool> {
        if project
            .ci_config_path
            .as_deref()
            .is_some_and(|path| !path.is_empty())
        {
            return Ok(true);
        }
        let url = format!(
            "{}/projects/{}/repository/files/.gitlab-ci.yml?ref=HEAD",
            self.api_url, project.id
        );
        match self.retry_policy.retry(|| self.get(&url)).await {
            Ok(_) => Ok(true),
            Err(PluginError::PipelineNotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub async fn get_pipeline(&self, project_id: i64, pipeline_id: i64) -> PluginResult<Pipeline> {
        self.retry_policy
            .retry(|| async {
                let response = self
                    .get(&format!(
                        "{}/projects/{}/pipelines/{}",
                        self.api_url, project_id, pipeline_id
                    ))
                    .await?;
                Self::json(response).await
            })
            .await
    }

    pub async fn trigger_pipeline(
        &self, project_id: i64, ref_name: String, variables: Option<Vec<PipelineVariable>>,
    ) -> PluginResult<Pipeline> {
        let url = Self::parse_url(&format!(
            "{}/projects/{}/pipeline",
            self.api_url, project_id
        ))?;
        let request_body = TriggerPipelineRequest {
            ref_name,
            variables,
        };

        let response = self
            .retry_policy
            .retry_unsent(&url, || {
                self.http_client
                    .post(url.clone())
                    .header("PRIVATE-TOKEN", &self.token)
                    .json(&request_body)
                    .send()
            })
            .await?;

        Self::json(check_response(response).await?).await
    }

    pub async fn cancel_pipeline(
        &self, project_id: i64, pipeline_id: i64,
    ) -> PluginResult<Pipeline> {
        let url = Self::parse_url(&format!(
            "{}/projects/{}/pipelines/{}/cancel",
            self.api_url, project_id, pipeline_id
        ))?;

        let response = self
            .retry_policy
            .retry_unsent(&url, || {
                self.http_client
                    .post(url.clone())
                    .header("PRIVATE-TOKEN", &self.token)
                    .send()
            })
            .await?;

        Self::json(check_response(response).await?).await
    }

    async fn project_page(
        &self, url: &str, params: &PaginationParams,
    ) -> PluginResult<PaginatedResponse<Project>> {
        let response = self.get(url).await?;
        let total_count = response
            .headers()
            .get("x-total")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok());

        let items: Vec<Project> = Self::json(response).await?;
        let count = total_count.unwrap_or(items.len());

        Ok(PaginatedResponse::new(
            items,
            params.page,
            params.page_size,
            count,
        ))
    }

    fn next_page(response: &reqwest::Response) -> Option<usize> {
        response
            .headers()
            .get("x-next-page")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok())
    }

    fn parse_url(url: &str) -> PluginResult<Url> {
        Url::parse(url).map_err(|e| PluginError::InvalidConfig(format!("Invalid GitLab URL: {e}")))
    }

    async fn get(&self, url: &str) -> PluginResult<reqwest::Response> {
        let url = Self::parse_url(url)?;
        let response = self
            .http_client
            .get(url.clone())
            .header("PRIVATE-TOKEN", &self.token)
            .send()
            .await
            .map_err(|e| map_send_error(e, &url))?;
        check_response(response).await
    }

    async fn json<T: serde::de::DeserializeOwned>(response: reqwest::Response) -> PluginResult<T> {
        let url = response.url().clone();
        response.json::<T>().await.map_err(|e| {
            let e = e.without_url();
            tracing::error!(error = %e, "Failed to deserialize GitLab response");
            PluginError::ApiError(format!(
                "Failed to parse GitLab API response from {}: {e}",
                pipedash_plugin_api::redact_url(&url),
            ))
        })
    }
}
