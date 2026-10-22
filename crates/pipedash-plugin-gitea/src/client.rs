use std::collections::HashMap;
use std::sync::{
    Arc,
    OnceLock,
};

use pipedash_plugin_api::{
    Organization,
    PaginatedResponse,
    PaginationParams,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use reqwest::{
    header::AUTHORIZATION,
    Method,
    StatusCode,
};
use serde::de::DeserializeOwned;

use crate::types::{
    Repository,
    Run,
    RunList,
    SearchResults,
    User,
    Workflow,
    WorkflowList,
};

const RUNS_PAGE_SIZE: usize = 50;
const MAX_RUN_PAGES: usize = 10;

pub(crate) struct GiteaClient {
    http_client: Arc<reqwest::Client>,
    base_url: String,
    token: String,
    retry_policy: RetryPolicy,
    user_cache: OnceLock<User>,
}

impl GiteaClient {
    pub fn new(http_client: Arc<reqwest::Client>, base_url: String, token: String) -> Self {
        Self {
            http_client,
            base_url,
            token,
            retry_policy: RetryPolicy::default(),
            user_cache: OnceLock::new(),
        }
    }

    pub fn actions_url(&self, owner: &str, repo: &str) -> String {
        format!("{}/{owner}/{repo}/actions", self.base_url)
    }

    pub async fn get_user(&self) -> PluginResult<User> {
        if let Some(user) = self.user_cache.get() {
            return Ok(user.clone());
        }
        let user: User = self.get("/user", &[]).await?;
        let _ = self.user_cache.set(user.clone());
        Ok(user)
    }

    pub async fn list_organizations(&self) -> PluginResult<Vec<Organization>> {
        let user = self.get_user().await?;
        let orgs: Vec<crate::types::Organization> = self.get("/user/orgs", &[]).await?;

        Ok(std::iter::once(Organization {
            id: user.id.to_string(),
            name: user.login,
            description: Some(user.full_name).filter(|name| !name.is_empty()),
        })
        .chain(orgs.into_iter().map(|org| Organization {
            id: org.id.to_string(),
            name: org.username,
            description: Some(org.description).filter(|d| !d.is_empty()),
        }))
        .collect())
    }

    pub async fn search_repositories(
        &self, uid: i64, exclusive: bool, search: Option<&str>, params: &PaginationParams,
    ) -> PluginResult<PaginatedResponse<Repository>> {
        let query = [
            ("uid", uid.to_string()),
            ("exclusive", exclusive.to_string()),
            ("q", search.unwrap_or_default().to_string()),
            ("page", params.page.to_string()),
            ("limit", params.page_size.to_string()),
        ];
        let (results, total): (SearchResults, _) =
            self.get_with_total("/repos/search", &query).await?;
        let total_count = total.unwrap_or(results.data.len());

        let mut response =
            PaginatedResponse::new(results.data, params.page, params.page_size, total_count);
        response.has_more = params.page * params.page_size < total_count;
        Ok(response)
    }

    pub async fn list_workflows(&self, owner: &str, repo: &str) -> PluginResult<Vec<Workflow>> {
        let list: WorkflowList = self
            .get(&format!("/repos/{owner}/{repo}/actions/workflows"), &[])
            .await?;
        Ok(list.workflows)
    }

    pub async fn list_runs(&self, owner: &str, repo: &str, page: usize) -> PluginResult<Vec<Run>> {
        let list: RunList = self
            .get(
                &format!("/repos/{owner}/{repo}/actions/runs"),
                &[
                    ("page", page.to_string()),
                    ("limit", RUNS_PAGE_SIZE.to_string()),
                ],
            )
            .await?;
        Ok(list.workflow_runs)
    }

    pub async fn latest_runs(
        &self, owner: &str, repo: &str, workflows: &[String],
    ) -> PluginResult<HashMap<String, Run>> {
        let mut latest = HashMap::new();
        for page in 1..=MAX_RUN_PAGES {
            let batch = self.list_runs(owner, repo, page).await?;
            let last_page = batch.len() < RUNS_PAGE_SIZE;
            for run in batch {
                let workflow = run.workflow_file().to_string();
                if workflows.contains(&workflow) {
                    latest.entry(workflow).or_insert(run);
                }
            }
            if latest.len() == workflows.len() || last_page {
                break;
            }
        }
        Ok(latest)
    }

    pub async fn workflow_runs(
        &self, owner: &str, repo: &str, workflow: &str, limit: usize,
    ) -> PluginResult<Vec<Run>> {
        let mut runs = Vec::new();
        for page in 1..=MAX_RUN_PAGES {
            let batch = self.list_runs(owner, repo, page).await?;
            let last_page = batch.len() < RUNS_PAGE_SIZE;
            runs.extend(
                batch
                    .into_iter()
                    .filter(|run| run.workflow_file() == workflow),
            );
            if runs.len() >= limit || last_page {
                break;
            }
        }
        runs.truncate(limit);
        Ok(runs)
    }

    pub async fn find_run(
        &self, owner: &str, repo: &str, workflow: &str, run_number: i64,
    ) -> PluginResult<Run> {
        for page in 1..=MAX_RUN_PAGES {
            let batch = self.list_runs(owner, repo, page).await?;
            let last_page = batch.len() < RUNS_PAGE_SIZE;
            if let Some(run) = batch
                .into_iter()
                .find(|run| run.run_number == run_number && run.workflow_file() == workflow)
            {
                return Ok(run);
            }
            if last_page {
                break;
            }
        }
        Err(PluginError::PipelineNotFound(format!(
            "Run #{run_number} not found for {owner}/{repo}/{workflow}"
        )))
    }

    pub async fn dispatch_workflow(
        &self, owner: &str, repo: &str, workflow: &str, body: &serde_json::Value,
    ) -> PluginResult<()> {
        let path = format!("/repos/{owner}/{repo}/actions/workflows/{workflow}/dispatches");
        self.retry_policy
            .retry(|| async {
                let response = self
                    .request(Method::POST, &path, &[])?
                    .json(body)
                    .send()
                    .await
                    .map_err(|e| {
                        PluginError::NetworkError(format!("Failed to dispatch workflow: {e}"))
                    })?;
                self.check_status(response).await.map(drop)
            })
            .await
    }

    pub async fn cancel_run(&self, owner: &str, repo: &str, run_id: i64) -> PluginResult<()> {
        let path = format!("/repos/{owner}/{repo}/actions/runs/{run_id}/cancel");
        let response = self
            .request(Method::POST, &path, &[])?
            .send()
            .await
            .map_err(|e| PluginError::NetworkError(format!("Failed to cancel run: {e}")))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(PluginError::NotSupported(
                "Run cancellation is not available on this Gitea version".to_string(),
            ));
        }
        self.check_status(response).await.map(drop)
    }

    fn request(
        &self, method: Method, path: &str, query: &[(&str, String)],
    ) -> PluginResult<reqwest::RequestBuilder> {
        let url =
            reqwest::Url::parse_with_params(&format!("{}/api/v1{path}", self.base_url), query)
                .map_err(|e| PluginError::InvalidConfig(format!("Invalid Gitea URL: {e}")))?;
        Ok(self
            .http_client
            .request(method, url)
            .header(AUTHORIZATION, format!("token {}", self.token)))
    }

    async fn get<T: DeserializeOwned>(
        &self, path: &str, query: &[(&str, String)],
    ) -> PluginResult<T> {
        self.get_with_total(path, query)
            .await
            .map(|(value, _)| value)
    }

    async fn get_with_total<T: DeserializeOwned>(
        &self, path: &str, query: &[(&str, String)],
    ) -> PluginResult<(T, Option<usize>)> {
        self.retry_policy
            .retry(|| async {
                let response = self
                    .request(Method::GET, path, query)?
                    .send()
                    .await
                    .map_err(|e| PluginError::NetworkError(format!("Failed to GET {path}: {e}")))?;
                let response = self.check_status(response).await?;
                let total = response
                    .headers()
                    .get("x-total-count")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse().ok());
                let url = response.url().clone();
                let value = response.json().await.map_err(|e| {
                    PluginError::ApiError(format!(
                        "Failed to parse Gitea API response from {url}: {e}"
                    ))
                })?;
                Ok((value, total))
            })
            .await
    }

    async fn check_status(&self, response: reqwest::Response) -> PluginResult<reqwest::Response> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let url = response.url().clone();
        if status == StatusCode::NOT_FOUND {
            return Err(PluginError::PipelineNotFound(format!(
                "Resource not found: {url}"
            )));
        }
        let body = response.text().await.unwrap_or_default();
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
            return Err(PluginError::AuthenticationFailed(format!(
                "Authentication failed for {url}: {body}"
            )));
        }
        Err(PluginError::ApiError(format!(
            "Gitea API error ({status}) for {url}: {body}"
        )))
    }
}
