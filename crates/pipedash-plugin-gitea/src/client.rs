use std::collections::HashMap;
use std::ops::{
    ControlFlow,
    Range,
};
use std::sync::Arc;

use pipedash_plugin_api::http::{
    api_url,
    decode_json,
    send,
};
use pipedash_plugin_api::utils::Cached;
use pipedash_plugin_api::{
    check_response,
    redact_url,
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
    Url,
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

mod logs;

const RUNS_PAGE_SIZE: usize = 50;
const MAX_RUN_PAGES: usize = 10;
const ORGS_PAGE_SIZE: usize = 50;

pub(crate) struct GiteaClient {
    http_client: Arc<reqwest::Client>,
    base_url: String,
    token: String,
    retry_policy: RetryPolicy,
    user_cache: Cached<User>,
}

impl GiteaClient {
    pub fn new(http_client: Arc<reqwest::Client>, base_url: String, token: String) -> Self {
        Self {
            http_client,
            base_url,
            token,
            retry_policy: RetryPolicy::default(),
            user_cache: Cached::default(),
        }
    }

    pub async fn get_user(&self) -> PluginResult<User> {
        self.user_cache
            .get_or_fetch(|| self.get("/user", &[]))
            .await
    }

    pub async fn list_organizations(&self) -> PluginResult<Vec<Organization>> {
        let user = self.get_user().await?;
        let mut orgs = Vec::new();
        for page in 1.. {
            let (batch, total): (Vec<crate::types::Organization>, _) = self
                .get_with_total(
                    "/user/orgs",
                    &[
                        ("page", page.to_string()),
                        ("limit", ORGS_PAGE_SIZE.to_string()),
                    ],
                )
                .await?;
            if batch.is_empty() {
                break;
            }
            orgs.extend(batch);
            if total.is_some_and(|total| orgs.len() >= total) {
                break;
            }
        }

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
        let start = (params.page - 1) * params.page_size;
        let (requested, total) = self
            .search_page(uid, exclusive, search, params.page, params.page_size)
            .await?;
        let mut items = requested.data;

        if let Some(total) = total.filter(|_| items.len() < params.page_size) {
            let limit = if params.page == 1 {
                items.len()
            } else {
                let (probe, _) = self
                    .search_page(uid, exclusive, search, 1, params.page_size)
                    .await?;
                probe.data.len()
            };
            if limit > 0 && limit < params.page_size.min(total) {
                let end = total.min(start + params.page_size);
                if params.page > 1 {
                    items.clear();
                }
                let from = start + items.len();
                if from < end {
                    items.extend(
                        self.search_window(uid, exclusive, search, from..end, limit)
                            .await?,
                    );
                }
            }
        }

        let total_count = total.unwrap_or(start + items.len());
        let mut response =
            PaginatedResponse::new(items, params.page, params.page_size, total_count);
        response.has_more = start + response.items.len() < total_count;
        Ok(response)
    }

    async fn search_window(
        &self, uid: i64, exclusive: bool, search: Option<&str>, window: Range<usize>, limit: usize,
    ) -> PluginResult<Vec<Repository>> {
        let mut items = Vec::with_capacity(window.len());
        let mut page = window.start / limit + 1;
        while (page - 1) * limit < window.end {
            let (batch, _) = self
                .search_page(uid, exclusive, search, page, limit)
                .await?;
            let batch_start = (page - 1) * limit;
            let batch_len = batch.data.len();
            items.extend(
                batch
                    .data
                    .into_iter()
                    .enumerate()
                    .filter(|(i, _)| window.contains(&(batch_start + i)))
                    .map(|(_, repo)| repo),
            );
            if batch_len < limit {
                break;
            }
            page += 1;
        }
        Ok(items)
    }

    async fn search_page(
        &self, uid: i64, exclusive: bool, search: Option<&str>, page: usize, limit: usize,
    ) -> PluginResult<(SearchResults, Option<usize>)> {
        let query = [
            ("uid", uid.to_string()),
            ("exclusive", exclusive.to_string()),
            ("q", search.unwrap_or_default().to_string()),
            ("page", page.to_string()),
            ("limit", limit.to_string()),
        ];
        self.get_with_total("/repos/search", &query).await
    }

    pub async fn list_workflows(&self, owner: &str, repo: &str) -> PluginResult<Vec<Workflow>> {
        let list: WorkflowList = self
            .get(&format!("/repos/{owner}/{repo}/actions/workflows"), &[])
            .await?;
        Ok(list.workflows)
    }

    async fn list_runs(&self, owner: &str, repo: &str, page: usize) -> PluginResult<RunList> {
        self.get(
            &format!("/repos/{owner}/{repo}/actions/runs"),
            &[
                ("page", page.to_string()),
                ("limit", RUNS_PAGE_SIZE.to_string()),
            ],
        )
        .await
    }

    pub async fn raw_file(&self, owner: &str, repo: &str, path: &str) -> PluginResult<String> {
        let path = format!("/repos/{owner}/{repo}/raw/{path}");
        self.retry_policy
            .retry(|| async {
                let response = self.get_response(&path, &[]).await?;
                let url = redact_url(response.url());
                response.text().await.map_err(|e| {
                    PluginError::ApiError(format!("Failed to read {url}: {}", e.without_url()))
                })
            })
            .await
    }

    async fn scan_runs<B>(
        &self, owner: &str, repo: &str, mut visit: impl FnMut(Vec<Run>) -> ControlFlow<B>,
    ) -> PluginResult<Option<B>> {
        let mut received = 0;
        for page in 1..=MAX_RUN_PAGES {
            let batch = self.list_runs(owner, repo, page).await?;
            received += batch.workflow_runs.len();
            let last_page = batch.is_last_page(received);
            if let ControlFlow::Break(found) = visit(batch.workflow_runs) {
                return Ok(Some(found));
            }
            if last_page {
                break;
            }
        }
        Ok(None)
    }

    pub async fn latest_runs(
        &self, owner: &str, repo: &str, workflows: &[String],
    ) -> PluginResult<HashMap<String, Run>> {
        let mut latest = HashMap::new();
        self.scan_runs(owner, repo, |runs| {
            for run in runs {
                let workflow = run.workflow_file().to_string();
                if workflows.contains(&workflow) {
                    latest.entry(workflow).or_insert(run);
                }
            }
            if latest.len() == workflows.len() {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .await?;
        Ok(latest)
    }

    pub async fn workflow_runs(
        &self, owner: &str, repo: &str, workflow: &str, limit: usize,
    ) -> PluginResult<Vec<Run>> {
        let mut found = Vec::new();
        self.scan_runs(owner, repo, |runs| {
            found.extend(
                runs.into_iter()
                    .filter(|run| run.workflow_file() == workflow),
            );
            if found.len() >= limit {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        })
        .await?;
        found.truncate(limit);
        Ok(found)
    }

    pub async fn find_run(
        &self, owner: &str, repo: &str, workflow: &str, run_number: i64,
    ) -> PluginResult<Run> {
        self.scan_runs(owner, repo, |runs| {
            match runs
                .into_iter()
                .find(|run| run.run_number == run_number && run.workflow_file() == workflow)
            {
                Some(run) => ControlFlow::Break(run),
                None => ControlFlow::Continue(()),
            }
        })
        .await?
        .ok_or_else(|| {
            PluginError::PipelineNotFound(format!(
                "Run #{run_number} not found for {owner}/{repo}/{workflow}"
            ))
        })
    }

    pub async fn dispatch_workflow(
        &self, owner: &str, repo: &str, workflow: &str, body: &serde_json::Value,
    ) -> PluginResult<()> {
        let path = format!("/repos/{owner}/{repo}/actions/workflows/{workflow}/dispatches");
        let url = self.url(&path, &[])?;
        let response = self
            .retry_policy
            .retry_unsent(&url, || self.request(Method::POST, &url).json(body).send())
            .await?;
        check_response(response).await.map(drop)
    }

    pub async fn cancel_run(&self, owner: &str, repo: &str, run_id: i64) -> PluginResult<()> {
        let path = format!("/repos/{owner}/{repo}/actions/runs/{run_id}/cancel");
        let url = self.url(&path, &[])?;
        let response = self
            .retry_policy
            .retry_unsent(&url, || self.request(Method::POST, &url).send())
            .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(PluginError::NotSupported(
                "Run cancellation is not available on this Gitea version".to_string(),
            ));
        }
        check_response(response).await.map(drop)
    }

    fn url(&self, path: &str, query: &[(&str, String)]) -> PluginResult<Url> {
        api_url(&format!("{}/api/v1", self.base_url), path, query, "Gitea")
    }

    fn request(&self, method: Method, url: &Url) -> reqwest::RequestBuilder {
        self.http_client
            .request(method, url.clone())
            .header(AUTHORIZATION, format!("token {}", self.token))
    }

    async fn get_response(
        &self, path: &str, query: &[(&str, String)],
    ) -> PluginResult<reqwest::Response> {
        let url = self.url(path, query)?;
        check_response(send(&url, self.request(Method::GET, &url)).await?).await
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
                let response = self.get_response(path, query).await?;
                let total = response
                    .headers()
                    .get("x-total-count")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse().ok());
                let value = decode_json(response, "Gitea API response").await?;
                Ok((value, total))
            })
            .await
    }
}
