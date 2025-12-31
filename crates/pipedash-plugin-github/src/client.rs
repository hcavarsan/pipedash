use std::collections::HashMap;

use chrono::Utc;
use futures::future::join_all;
use octocrab::Octocrab;
use pipedash_plugin_api::{
    AvailablePipeline,
    PaginatedResponse,
    PaginationParams,
    PermissionStatus,
    Pipeline,
    PipelineRun,
    PluginError,
    PluginResult,
    RetryPolicy,
};
use tracing::debug;

use crate::{
    config,
    mapper,
    permissions::PermissionChecker,
    types,
};

pub(crate) struct GitHubClient {
    pub(crate) octocrab: Octocrab,
    pub(crate) retry_policy: RetryPolicy,
    permission_checker: PermissionChecker,
}

impl GitHubClient {
    pub fn new(octocrab: Octocrab, token: String) -> PluginResult<Self> {
        let token_secret = token.into();
        let permission_checker = PermissionChecker::new(octocrab.clone(), token_secret)?;

        Ok(Self {
            octocrab,
            retry_policy: RetryPolicy::default(),
            permission_checker,
        })
    }

    async fn detect_organizations_from_repos(
        &self,
    ) -> PluginResult<Vec<pipedash_plugin_api::Organization>> {
        use std::collections::HashMap;

        tracing::debug!("Detecting organizations from accessible repositories");

        let repos = self
            .octocrab
            .current()
            .list_repos_for_authenticated_user()
            .per_page(100)
            .send()
            .await
            .map_err(|e| {
                PluginError::ApiError(format!("Failed to fetch accessible repositories: {e}"))
            })?;

        let mut org_map: HashMap<String, pipedash_plugin_api::Organization> = HashMap::new();

        for repo in repos.items {
            if let Some(owner) = repo.owner {
                if !org_map.contains_key(&owner.login) {
                    org_map.insert(
                        owner.login.clone(),
                        pipedash_plugin_api::Organization {
                            id: owner.login.clone(),
                            name: owner.login,
                            description: None,
                        },
                    );
                }
            }
        }

        let organizations: Vec<_> = org_map.into_values().collect();
        tracing::debug!(
            "Detected {} organizations from repositories",
            organizations.len()
        );

        Ok(organizations)
    }

    pub async fn fetch_organizations(
        &self,
    ) -> PluginResult<Vec<pipedash_plugin_api::Organization>> {
        self.retry_policy
            .retry(|| async {
                let mut orgs = Vec::new();

                let user = self.octocrab.current().user().await.map_err(|e| {
                    PluginError::ApiError(format!("Failed to fetch current user: {e}"))
                })?;

                orgs.push(pipedash_plugin_api::Organization {
                    id: user.login.clone(),
                    name: user.login.clone(),
                    description: user.name.clone(),
                });

                let org_result = self
                    .octocrab
                    .current()
                    .list_org_memberships_for_authenticated_user()
                    .per_page(100)
                    .send()
                    .await;

                match org_result {
                    Ok(organizations) => {
                        tracing::debug!("Successfully fetched organization memberships");
                        for org in organizations.items {
                            orgs.push(pipedash_plugin_api::Organization {
                                id: org.organization.login.clone(),
                                name: org.organization.login.clone(),
                                description: org.organization.description.clone(),
                            });
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Cannot fetch organization memberships ({}). Falling back to repository-based detection.",
                            e
                        );

                        match self.detect_organizations_from_repos().await {
                            Ok(detected_orgs) => {
                                tracing::debug!("Successfully detected organizations from repositories");
                                for org in detected_orgs {
                                    if org.id != user.login {
                                        orgs.push(org);
                                    }
                                }
                            }
                            Err(detect_err) => {
                                tracing::warn!(
                                    "Failed to detect organizations from repositories: {}. \
                                    Only personal account will be available.",
                                    detect_err
                                );
                            }
                        }
                    }
                }

                Ok(orgs)
            })
            .await
    }

    pub async fn fetch_all_repositories(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let params = params.unwrap_or_default();
        let github_page = params.page.min(255) as u8;

        let repos = self
            .octocrab
            .current()
            .list_repos_for_authenticated_user()
            .per_page(params.page_size.min(100) as u8)
            .page(github_page)
            .send()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to fetch repositories: {e}")))?;

        let mut all_repos = Vec::new();
        for repo in repos.items {
            let full_name = repo
                .full_name
                .as_ref()
                .ok_or_else(|| PluginError::ApiError("Repository missing full_name".to_string()))?;

            let (owner, repo_name) = if let Some((o, r)) = config::parse_repo(full_name) {
                (Some(o), Some(r))
            } else {
                (None, None)
            };

            all_repos.push(AvailablePipeline {
                id: full_name.clone(),
                name: full_name.clone(),
                description: repo.description.clone(),
                organization: owner,
                repository: repo_name,
            });
        }

        let total_count = repos
            .total_count
            .map(|t| t as usize)
            .unwrap_or(all_repos.len());
        Ok(PaginatedResponse::new(
            all_repos,
            params.page,
            params.page_size,
            total_count,
        ))
    }

    pub async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let params = params.unwrap_or_default();
        let github_page = params.page.min(255) as u8;

        let mut all_repos = Vec::new();

        if let Some(org_name) = org {
            let user =
                self.octocrab.current().user().await.map_err(|e| {
                    PluginError::ApiError(format!("Failed to fetch current user: {e}"))
                })?;

            let is_personal_account = org_name == user.login;

            let repos = if is_personal_account {
                self.octocrab
                    .current()
                    .list_repos_for_authenticated_user()
                    .per_page(params.page_size.min(100) as u8)
                    .page(github_page)
                    .send()
                    .await
                    .map_err(|e| {
                        PluginError::ApiError(format!("Failed to fetch user repositories: {e}"))
                    })?
            } else {
                self.octocrab
                    .orgs(&org_name)
                    .list_repos()
                    .per_page(params.page_size.min(100) as u8)
                    .page(github_page)
                    .send()
                    .await
                    .map_err(|e| {
                        PluginError::ApiError(format!("Failed to fetch org repositories: {e}"))
                    })?
            };

            for repo in repos.items {
                let full_name = repo.full_name.as_ref().ok_or_else(|| {
                    PluginError::ApiError("Repository missing full_name".to_string())
                })?;

                let (owner, repo_name) = if let Some((o, r)) = config::parse_repo(full_name) {
                    (Some(o), Some(r))
                } else {
                    (None, None)
                };

                if is_personal_account {
                    if let Some(ref owner_name) = owner {
                        if owner_name != &user.login {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }

                all_repos.push(AvailablePipeline {
                    id: full_name.clone(),
                    name: full_name.clone(),
                    description: repo.description.clone(),
                    organization: owner,
                    repository: repo_name,
                });
            }

            if let Some(search_term) = search {
                let search_lower = search_term.to_lowercase();
                all_repos.retain(|p| {
                    p.name.to_lowercase().contains(&search_lower)
                        || p.id.to_lowercase().contains(&search_lower)
                        || p.description
                            .as_ref()
                            .is_some_and(|d| d.to_lowercase().contains(&search_lower))
                });
            }

            let total_count = repos
                .total_count
                .map(|t| t as usize)
                .unwrap_or(all_repos.len());
            return Ok(PaginatedResponse::new(
                all_repos,
                params.page,
                params.page_size,
                total_count,
            ));
        }

        let repos = self
            .octocrab
            .current()
            .list_repos_for_authenticated_user()
            .per_page(params.page_size.min(100) as u8)
            .page(github_page)
            .send()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to fetch repositories: {e}")))?;

        for repo in repos.items {
            let full_name = repo
                .full_name
                .as_ref()
                .ok_or_else(|| PluginError::ApiError("Repository missing full_name".to_string()))?;

            let (owner, repo_name) = if let Some((o, r)) = config::parse_repo(full_name) {
                (Some(o), Some(r))
            } else {
                (None, None)
            };

            all_repos.push(AvailablePipeline {
                id: full_name.clone(),
                name: full_name.clone(),
                description: repo.description.clone(),
                organization: owner,
                repository: repo_name,
            });
        }

        if let Some(search_term) = search {
            let search_lower = search_term.to_lowercase();
            all_repos.retain(|p| {
                p.name.to_lowercase().contains(&search_lower)
                    || p.id.to_lowercase().contains(&search_lower)
                    || p.description
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&search_lower))
            });
        }

        let total_count = repos
            .total_count
            .map(|t| t as usize)
            .unwrap_or(all_repos.len());
        Ok(PaginatedResponse::new(
            all_repos,
            params.page,
            params.page_size,
            total_count,
        ))
    }

    pub async fn fetch_repo_workflows(
        &self, provider_id: i64, repo_full_name: String,
    ) -> PluginResult<Vec<Pipeline>> {
        self.retry_policy
            .retry(|| async {
                let (owner, repo) = config::parse_repo(&repo_full_name).ok_or_else(|| {
                    PluginError::InvalidConfig(format!(
                        "Invalid repository format: {repo_full_name}"
                    ))
                })?;

                let workflows = self
                    .octocrab
                    .workflows(&owner, &repo)
                    .list()
                    .per_page(100)
                    .send()
                    .await
                    .map_err(|e| {
                        PluginError::ApiError(format!("Failed to fetch workflows: {e}"))
                    })?;

                let fetch_runs_futures = workflows.items.iter().map(|workflow| {
                    let octocrab = self.octocrab.clone();
                    let owner = owner.clone();
                    let repo = repo.clone();
                    let workflow_id = workflow.id;
                    async move {
                        let runs = octocrab
                            .workflows(&owner, &repo)
                            .list_runs(workflow_id.to_string())
                            .per_page(1)
                            .send()
                            .await
                            .ok();
                        (workflow_id, runs)
                    }
                });

                let runs_results: Vec<_> = join_all(fetch_runs_futures).await;
                let runs_map: std::collections::HashMap<_, _> = runs_results.into_iter().collect();

                let mut pipelines = Vec::new();

                for workflow in workflows.items {
                    let runs = runs_map.get(&workflow.id);
                    let latest_run = runs.and_then(|r| r.as_ref()).and_then(|r| r.items.first());

                    let status = latest_run
                        .map(|run| {
                            mapper::map_status(run.status.as_str(), run.conclusion.as_deref())
                        })
                        .unwrap_or(pipedash_plugin_api::PipelineStatus::Pending);

                    let last_run = latest_run.map(|run| run.created_at.with_timezone(&Utc));

                    let mut metadata = HashMap::new();
                    metadata.insert("workflow_id".to_string(), serde_json::json!(workflow.id));

                    let pipeline = Pipeline {
                        id: format!(
                            "github__{}__{}__{}__{}",
                            provider_id, owner, repo, workflow.id
                        ),
                        provider_id,
                        provider_type: "github".to_string(),
                        name: workflow.name,
                        status,
                        last_run,
                        last_updated: Utc::now(),
                        repository: repo_full_name.clone(),
                        branch: latest_run.map(|run| run.head_branch.clone()),
                        workflow_file: Some(workflow.path),
                        metadata,
                    };

                    pipelines.push(pipeline);
                }

                Ok(pipelines)
            })
            .await
    }

    pub async fn fetch_run_history(
        &self, owner: &str, repo: &str, workflow_id: u64, limit: usize,
    ) -> PluginResult<Vec<types::Run>> {
        use std::sync::Arc;

        use tokio::sync::Semaphore;

        let per_page = 100u8;
        let total_pages = limit.div_ceil(100).min(10);

        const MAX_CONCURRENT: usize = 5;
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));

        let page_futures: Vec<_> = (1..=total_pages)
            .map(|page| {
                let octocrab = self.octocrab.clone();
                let owner = owner.to_string();
                let repo = repo.to_string();
                let workflow_id_str = workflow_id.to_string();
                let semaphore = semaphore.clone();

                async move {
                    let _permit = semaphore.acquire().await.unwrap();

                    octocrab
                        .workflows(&owner, &repo)
                        .list_runs(workflow_id_str)
                        .per_page(per_page)
                        .page(page as u8)
                        .send()
                        .await
                        .map(|response| response.items)
                        .map_err(|e| {
                            PluginError::ApiError(format!(
                                "Failed to fetch run history page {page}: {e}"
                            ))
                        })
                }
            })
            .collect();

        let results = join_all(page_futures).await;

        let mut all_runs = Vec::new();
        for result in results {
            all_runs.extend(result?);
        }

        all_runs.truncate(limit);
        Ok(all_runs)
    }

    pub async fn fetch_run_by_number(
        &self, owner: &str, repo: &str, workflow_id: u64, run_number: i64,
    ) -> PluginResult<types::Run> {
        let mut page = 1u32;

        loop {
            let runs = self
                .octocrab
                .workflows(owner, repo)
                .list_runs(workflow_id.to_string())
                .per_page(100)
                .page(page.min(255) as u8)
                .send()
                .await
                .map_err(|e| PluginError::ApiError(format!("Failed to fetch runs: {e}")))?;

            if let Some(run) = runs.items.into_iter().find(|r| r.run_number == run_number) {
                return Ok(run);
            }

            if page >= 10 {
                break;
            }

            page += 1;
        }

        Err(PluginError::PipelineNotFound(format!(
            "Run #{run_number} not found"
        )))
    }

    pub async fn cancel_run(&self, owner: &str, repo: &str, run_id: u64) -> PluginResult<()> {
        let owner = owner.to_string();
        let repo = repo.to_string();

        self.retry_policy
            .retry(|| async {
                debug!("Cancelling run {run_id} for {owner}/{repo}");

                let url = format!("/repos/{owner}/{repo}/actions/runs/{run_id}/cancel");

                let response: Result<serde_json::Value, octocrab::Error> =
                    self.octocrab.post(url, None::<&()>).await;

                match response {
                    Ok(_) => {
                        debug!("Run {run_id} cancelled successfully");
                        Ok(())
                    }
                    Err(e) => {
                        debug!("Cancel failed: {e}");
                        Err(PluginError::ApiError(format!("Failed to cancel run: {e}")))
                    }
                }
            })
            .await
    }

    pub async fn check_token_permissions(&self) -> PluginResult<PermissionStatus> {
        self.permission_checker.check_token_permissions().await
    }
}

pub(crate) fn run_to_pipeline_run(run: types::Run, pipeline_id: &str) -> PipelineRun {
    let status = mapper::map_status(run.status.as_str(), run.conclusion.as_deref());

    let duration_seconds = {
        let started = run.created_at;
        let concluded = run.updated_at;
        Some((concluded - started).num_seconds())
    };

    let mut inputs_map = serde_json::Map::new();

    inputs_map.insert(
        "ref".to_string(),
        serde_json::Value::String(run.head_branch.clone()),
    );

    debug!(
        "Run #{}: branch={}, event={:?}",
        run.run_number, run.head_branch, run.event
    );

    let inputs = Some(serde_json::Value::Object(inputs_map));

    let mut metadata = HashMap::new();
    metadata.insert("event".to_string(), serde_json::json!(&run.event));
    metadata.insert("run_id".to_string(), serde_json::json!(run.id.0));

    if let Some(owner) = run.repository.owner.as_ref() {
        metadata.insert("owner".to_string(), serde_json::json!(&owner.login));
    }

    PipelineRun {
        id: format!("github-run-{}", run.id),
        pipeline_id: pipeline_id.to_string(),
        run_number: run.run_number,
        status,
        started_at: run.created_at.with_timezone(&Utc),
        concluded_at: Some(run.updated_at.with_timezone(&Utc)),
        duration_seconds,
        logs_url: run.html_url.to_string(),
        commit_sha: Some(run.head_sha.clone()),
        commit_message: Some(run.head_commit.message.clone()),
        branch: Some(run.head_branch.clone()),
        actor: Some(run.head_commit.author.name.clone()),
        inputs,
        metadata,
    }
}
