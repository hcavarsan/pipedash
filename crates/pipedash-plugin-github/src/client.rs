//! GitHub API client and methods

use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use futures::future::join_all;
use octocrab::Octocrab;
use pipedash_plugin_api::{
    AvailablePipeline,
    Pipeline,
    PipelineRun,
    PluginError,
    PluginResult,
};

use crate::{
    config,
    mapper,
    types,
};

/// GitHub API client with retry logic
pub(crate) struct GitHubClient {
    pub(crate) octocrab: Octocrab,
}

impl GitHubClient {
    pub fn new(octocrab: Octocrab) -> Self {
        Self { octocrab }
    }

    /// Retries a request operation with exponential backoff
    async fn retry_request<F, Fut, T>(&self, operation: F) -> PluginResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = PluginResult<T>>,
    {
        let max_retries = 3;
        let mut delay = Duration::from_millis(100);
        let mut last_error = None;

        for attempt in 0..max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) if attempt < max_retries - 1 => match &e {
                    PluginError::NetworkError(_) | PluginError::ApiError(_) => {
                        last_error = Some(e);
                        tokio::time::sleep(delay).await;
                        delay *= 2;
                        continue;
                    }
                    _ => return Err(e),
                },
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| PluginError::NetworkError("Max retries exceeded".to_string())))
    }

    /// Fetches all repositories the authenticated user has access to
    pub async fn fetch_all_repositories(&self) -> PluginResult<Vec<AvailablePipeline>> {
        let mut page = 1u32;
        let mut all_repos = Vec::new();

        loop {
            let repos = self
                .octocrab
                .current()
                .list_repos_for_authenticated_user()
                .per_page(100)
                .page(page.min(255) as u8)
                .send()
                .await
                .map_err(|e| PluginError::ApiError(format!("Failed to fetch repositories: {e}")))?;

            if repos.items.is_empty() {
                break;
            }

            for repo in repos.items {
                let full_name = repo.full_name.as_ref().ok_or_else(|| {
                    PluginError::ApiError("Repository missing full_name".to_string())
                })?;

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

            page += 1;
            if page > 10 {
                break;
            }
        }

        Ok(all_repos)
    }

    /// Fetches all workflows for a repository
    pub async fn fetch_repo_workflows(
        &self, provider_id: i64, repo_full_name: String,
    ) -> PluginResult<Vec<Pipeline>> {
        self.retry_request(|| async {
            let (owner, repo) = config::parse_repo(&repo_full_name).ok_or_else(|| {
                PluginError::InvalidConfig(format!("Invalid repository format: {repo_full_name}"))
            })?;

            let workflows = self
                .octocrab
                .workflows(&owner, &repo)
                .list()
                .per_page(100)
                .send()
                .await
                .map_err(|e| PluginError::ApiError(format!("Failed to fetch workflows: {e}")))?;

            // Fetch latest run for each workflow in parallel
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
                    .map(|run| mapper::map_status(run.status.as_str(), run.conclusion.as_deref()))
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

    /// Fetches run history for a workflow
    pub async fn fetch_run_history(
        &self, owner: &str, repo: &str, workflow_id: u64, limit: usize,
    ) -> PluginResult<Vec<types::Run>> {
        let per_page = 100u8;
        let total_pages = limit.div_ceil(100);
        let mut all_runs = Vec::new();

        for page in 1..=total_pages.min(10) {
            let runs = self
                .octocrab
                .workflows(owner, repo)
                .list_runs(workflow_id.to_string())
                .per_page(per_page)
                .page(page as u8)
                .send()
                .await
                .map_err(|e| PluginError::ApiError(format!("Failed to fetch run history: {e}")))?;

            if runs.items.is_empty() {
                break;
            }

            all_runs.extend(runs.items);

            if all_runs.len() >= limit {
                break;
            }
        }

        all_runs.truncate(limit);
        Ok(all_runs)
    }

    /// Fetches a specific run by number
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

    /// Cancels a workflow run
    pub async fn cancel_run(&self, owner: &str, repo: &str, run_id: u64) -> PluginResult<()> {
        eprintln!("[GITHUB] Cancelling run {run_id} for {owner}/{repo}");

        // Use octocrab's execute method to make a raw request
        // GitHub's cancel endpoint returns 202 with an empty object
        let url = format!("/repos/{owner}/{repo}/actions/runs/{run_id}/cancel");

        // Make the request and accept any JSON response
        let response: Result<serde_json::Value, octocrab::Error> =
            self.octocrab.post(url, None::<&()>).await;

        match response {
            Ok(_) => {
                eprintln!("[GITHUB] Run {run_id} cancelled successfully");
                Ok(())
            }
            Err(e) => {
                eprintln!("[GITHUB] Cancel failed: {e}");
                Err(PluginError::ApiError(format!("Failed to cancel run: {e}")))
            }
        }
    }
}

/// Converts GitHub Actions Run to PipelineRun
pub(crate) fn run_to_pipeline_run(run: types::Run, pipeline_id: &str) -> PipelineRun {
    let status = mapper::map_status(run.status.as_str(), run.conclusion.as_deref());

    let duration_seconds = {
        let started = run.created_at;
        let concluded = run.updated_at;
        Some((concluded - started).num_seconds())
    };

    // Extract inputs for replay functionality
    // For GitHub Actions, the most important input is the ref (branch/tag/commit)
    // We always include this so replays can rerun on the same branch
    let mut inputs_map = serde_json::Map::new();

    // Always include the ref (branch/tag) used in the original run
    inputs_map.insert(
        "ref".to_string(),
        serde_json::Value::String(run.head_branch.clone()),
    );

    eprintln!(
        "[GITHUB] Run #{}: branch={}, event={:?}",
        run.run_number, run.head_branch, run.event
    );

    let inputs = Some(serde_json::Value::Object(inputs_map));

    // Populate GitHub-specific metadata
    let mut metadata = HashMap::new();
    metadata.insert("event".to_string(), serde_json::json!(&run.event));
    metadata.insert("run_id".to_string(), serde_json::json!(run.id.0));

    // Extract owner from repository for organization column
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
