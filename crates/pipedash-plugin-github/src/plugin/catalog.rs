use pipedash_plugin_api::support::join_buffered;
use pipedash_plugin_api::*;

use super::GitHubPlugin;
use crate::{
    config,
    mapper,
};

impl GitHubPlugin {
    pub(super) async fn fetch_pipelines_impl(&self) -> PluginResult<Vec<Pipeline>> {
        let repositories = config::get_repositories(&self.config);
        tracing::debug!(repositories = ?repositories, "Configured GitHub repositories");

        if repositories.is_empty() {
            return Err(PluginError::InvalidConfig(
                "No repositories configured".to_string(),
            ));
        }

        let client = self.client()?;
        let host = config::source_host(&self.config);
        let requests: Vec<_> = repositories
            .iter()
            .map(|repo_full_name| {
                client.fetch_repo_workflows(repo_full_name.clone(), host.as_deref())
            })
            .collect();
        let results: Vec<_> = join_buffered(requests, 8).await;

        let mut all_pipelines = Vec::new();
        let mut first_error = None;

        for (repo_full_name, result) in repositories.iter().zip(results) {
            match result {
                Ok(mut pipelines) => {
                    tracing::debug!(count = pipelines.len(), "GitHub repo returned workflows");
                    all_pipelines.append(&mut pipelines);
                }
                Err(e) if e.is_transient() => return Err(e),
                Err(e) => {
                    tracing::warn!(
                        repository = %repo_full_name,
                        error = %e,
                        "Skipping GitHub repository after a non-transient failure"
                    );
                    first_error.get_or_insert(e);
                }
            }
        }

        let unique_count = all_pipelines
            .iter()
            .map(|p| &p.id)
            .collect::<std::collections::HashSet<_>>()
            .len();
        tracing::debug!(
            unique_pipelines = unique_count,
            total_pipelines = all_pipelines.len(),
            "GitHub pipeline fetch complete"
        );

        match first_error {
            Some(e) if all_pipelines.is_empty() => Err(e),
            _ => Ok(all_pipelines),
        }
    }

    fn run_source(&self, owner: &str, repo: &str) -> (Option<String>, Option<RepoId>) {
        let host = config::source_host(&self.config);
        let repo_id = host
            .as_deref()
            .and_then(|host| RepoId::from_host_path(host, &format!("{owner}/{repo}")));
        (host, repo_id)
    }

    pub(super) async fn fetch_run_history_impl(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let (owner, repo, workflow_id) = Self::pipeline_ref(pipeline_id)?;
        let client = self.client()?;

        let runs = client
            .fetch_run_history(owner, repo, workflow_id, limit)
            .await?;
        let (host, repo_id) = self.run_source(owner, repo);
        let targets = client
            .run_targets(owner, repo, host.as_deref(), &runs)
            .await?;

        Ok(runs
            .into_iter()
            .zip(targets)
            .map(|(run, targets)| {
                mapper::run_to_pipeline_run(run, pipeline_id, repo_id.clone(), targets)
            })
            .collect())
    }

    pub(super) async fn fetch_run_details_impl(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let (owner, repo, workflow_id) = Self::pipeline_ref(pipeline_id)?;
        let client = self.client()?;

        let run = client
            .fetch_run_by_number(owner, repo, workflow_id, run_number)
            .await?;
        let (host, repo_id) = self.run_source(owner, repo);
        let targets = client
            .run_targets(owner, repo, host.as_deref(), std::slice::from_ref(&run))
            .await?
            .pop()
            .unwrap_or_default();

        Ok(mapper::run_to_pipeline_run(
            run,
            pipeline_id,
            repo_id,
            targets,
        ))
    }
}
