use pipedash_plugin_actions::dispatch_inputs;
use pipedash_plugin_api::support::string_parameter;
use pipedash_plugin_api::*;

use super::GitHubPlugin;
use crate::mapper;

impl GitHubPlugin {
    pub(super) async fn fetch_log_nodes_impl(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let (owner, repo) = Self::repo_of(run.pipeline_id)?;
        let run_id = run
            .metadata
            .get("run_id")
            .and_then(serde_json::Value::as_u64)
            .or_else(|| {
                run.run_id
                    .strip_prefix("github-run-")
                    .and_then(|id| id.parse().ok())
            })
            .ok_or_else(|| {
                PluginError::InvalidConfig(format!("Run #{} has no GitHub run ID", run.run_number))
            })?;

        let jobs = self.client()?.list_run_jobs(owner, repo, run_id).await?;

        Ok(mapper::jobs_to_log_nodes(&jobs))
    }

    pub(super) async fn read_log_impl(
        &self, run: RunRef<'_>, node_id: &str, _from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let (owner, repo) = Self::repo_of(run.pipeline_id)?;
        let job_id: u64 = node_id
            .parse()
            .map_err(|_| PluginError::InvalidConfig("Only jobs have logs on GitHub".into()))?;

        self.client()?
            .stream_job_log(owner, repo, job_id, sink)
            .await
            .map(|()| LogRead::replace(None))
    }

    pub(super) async fn fetch_workflow_parameters_impl(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        let (owner, repo, workflow_number) = Self::pipeline_ref(workflow_id)?;

        let source = self
            .client()?
            .fetch_workflow_source(owner, repo, workflow_number)
            .await?;

        let mut parameters = vec![string_parameter(
            "ref",
            "Ref",
            "Branch, tag, or commit SHA to run workflow on",
            Some("main".to_string()),
            true,
        )];
        parameters.extend(dispatch_inputs(&source)?);
        Ok(parameters)
    }
}
