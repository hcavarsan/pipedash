use std::collections::HashMap;

use chrono::Utc;
use futures::stream::{
    self,
    StreamExt,
};
use octocrab::models::workflows::{
    Run,
    WorkFlow,
};
use octocrab::models::WorkflowId;
use pipedash_plugin_api::{
    Pipeline,
    PipelineStatus,
    PluginError,
    PluginResult,
};

use super::{
    map_octocrab_error,
    GitHubClient,
};
use crate::{
    config,
    mapper,
};

fn workflow_pipeline(
    owner: &str, repo: &str, repo_full_name: &str, workflow: WorkFlow, latest_run: Option<&Run>,
) -> Pipeline {
    let resource_state = mapper::disabled_state(&workflow.state);
    let status = if resource_state.is_some() {
        PipelineStatus::Skipped
    } else {
        latest_run
            .map(|run| mapper::map_status(run.status.as_str(), run.conclusion.as_deref()))
            .unwrap_or(PipelineStatus::Pending)
    };

    let mut metadata = HashMap::new();
    metadata.insert("workflow_id".to_string(), serde_json::json!(workflow.id));
    if resource_state.is_some() {
        metadata.insert("can_enable".to_string(), serde_json::json!(true));
    }

    Pipeline {
        id: format!("{owner}__{repo}__{}", workflow.id),
        name: workflow.name,
        status,
        last_run: latest_run.map(|run| run.created_at.with_timezone(&Utc)),
        last_updated: Utc::now(),
        repository: repo_full_name.to_string(),
        branch: latest_run.map(|run| run.head_branch.clone()),
        workflow_file: Some(workflow.path),
        resource_state,
        metadata,
    }
}

impl GitHubClient {
    async fn latest_runs(
        &self, owner: &str, repo: &str, repo_full_name: &str, workflows: &[WorkFlow],
    ) -> PluginResult<HashMap<WorkflowId, octocrab::Page<Run>>> {
        let requests: Vec<_> = workflows
            .iter()
            .map(|workflow| {
                let octocrab = self.octocrab.clone();
                let owner = owner.to_string();
                let repo = repo.to_string();
                let workflow_id = workflow.id;
                async move {
                    let runs = octocrab
                        .workflows(&owner, &repo)
                        .list_runs(workflow_id.to_string())
                        .per_page(1)
                        .send()
                        .await;
                    (workflow_id, runs)
                }
            })
            .collect();
        let results: Vec<_> = stream::iter(requests).buffered(8).collect().await;

        let mut runs_map = HashMap::new();
        for (workflow_id, result) in results {
            let runs = match result {
                Ok(runs) => runs,
                Err(e) => {
                    let e = map_octocrab_error("Failed to fetch latest run", e);
                    if e.is_transient() {
                        return Err(e);
                    }
                    tracing::warn!(
                        repository = %repo_full_name,
                        workflow_id = %workflow_id,
                        error = %e,
                        "Latest run unavailable, reporting the workflow as pending"
                    );
                    continue;
                }
            };
            runs_map.insert(workflow_id, runs);
        }
        Ok(runs_map)
    }

    pub async fn fetch_repo_workflows(
        &self, repo_full_name: String,
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
                    .map_err(|e| map_octocrab_error("Failed to fetch workflows", e))?;

                let runs_map = self
                    .latest_runs(&owner, &repo, &repo_full_name, &workflows.items)
                    .await?;

                Ok(workflows
                    .items
                    .into_iter()
                    .map(|workflow| {
                        let latest_run = runs_map
                            .get(&workflow.id)
                            .and_then(|runs| runs.items.first());
                        workflow_pipeline(&owner, &repo, &repo_full_name, workflow, latest_run)
                    })
                    .collect())
            })
            .await
    }

    pub async fn has_workflows(&self, owner: &str, repo: &str) -> PluginResult<bool> {
        self.retry_policy
            .retry(|| async {
                let page = self
                    .octocrab
                    .workflows(owner, repo)
                    .list()
                    .per_page(1)
                    .send()
                    .await
                    .map_err(|e| map_octocrab_error("Failed to list workflows", e))?;
                Ok(!page.items.is_empty())
            })
            .await
    }

    pub async fn fetch_workflow_source(
        &self, owner: &str, repo: &str, workflow_id: u64,
    ) -> PluginResult<String> {
        let url = format!("/repos/{owner}/{repo}/actions/workflows/{workflow_id}");

        self.retry_policy
            .retry(|| async {
                let workflow: WorkFlow = self
                    .octocrab
                    .get(&url, None::<&()>)
                    .await
                    .map_err(|e| map_octocrab_error("Failed to fetch workflow", e))?;

                let response = self
                    .octocrab
                    .repos(owner, repo)
                    .get_content()
                    .path(&workflow.path)
                    .raw()
                    .send()
                    .await
                    .map_err(|e| match map_octocrab_error("Failed to fetch workflow file", e) {
                        PluginError::PipelineNotFound(_) => PluginError::PipelineNotFound(format!(
                            "{} isn't on the default branch, so this workflow can't be triggered. GitHub keeps listing a workflow after its file is deleted or when it only ran from another branch. On a private repository, a fine-grained token without Contents (Read) gets the same response.",
                            workflow.path
                        )),
                        PluginError::PermissionDenied(message) => PluginError::PermissionDenied(
                            format!("{message}. Fine-grained tokens need Contents (Read) on the repository"),
                        ),
                        other => other,
                    })?;

                self.octocrab
                    .body_to_string(response)
                    .await
                    .map_err(|e| map_octocrab_error("Failed to read workflow file", e))
            })
            .await
    }
}
