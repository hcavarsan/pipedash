use std::collections::HashMap;
use std::sync::Arc;

use pipedash_plugin_actions::{
    dispatch_body,
    dispatch_inputs,
};
use pipedash_plugin_api::config::{
    required_value,
    selected_items,
};
use pipedash_plugin_api::support::{
    digit_id,
    require_configured,
    resume_position,
    string_parameter,
    try_buffered,
    CONCURRENCY,
};
use pipedash_plugin_api::*;

use crate::log_dialect::GiteaLogDialect;
use crate::{
    client,
    config,
    mapper,
    metadata,
    types::JobAttempt,
};

#[derive(serde::Serialize, serde::Deserialize)]
struct LogPosition {
    job: String,
    attempt: JobAttempt,
    offset: u64,
}

plugin_struct!(
    GiteaPlugin,
    client::GiteaClient,
    metadata::create_metadata();
    status_source: Option<StatusSource> = None
);

impl GiteaPlugin {
    async fn fetch_repo_pipelines(&self, repo_full_name: &str) -> PluginResult<Vec<Pipeline>> {
        let (owner, repo) = config::parse_repo(repo_full_name)?;
        let client = self.client()?;
        let workflows = client.list_workflows(owner, repo).await?;
        let workflow_ids: Vec<String> = workflows.iter().map(|w| w.id.clone()).collect();
        let latest_runs = client.latest_runs(owner, repo, &workflow_ids).await?;

        Ok(workflows
            .iter()
            .map(|workflow| {
                let latest_run = latest_runs.get(&workflow.id);
                mapper::map_pipeline(owner, repo, workflow, latest_run)
            })
            .collect())
    }

    async fn fetch_pipelines_impl(&self) -> PluginResult<Vec<Pipeline>> {
        let repositories = require_configured(
            selected_items(&self.config).unwrap_or_default(),
            "repositories",
        )?;

        let requests: Vec<_> = repositories
            .iter()
            .map(|repo| self.fetch_repo_pipelines(repo))
            .collect();
        let results = try_buffered(requests, CONCURRENCY).await?;

        Ok(results.into_iter().flatten().collect())
    }
}

plugin_impl!(GiteaPlugin [metadata, status_source, available_from_filtered, fetch_pipelines] {
    fn initialize(
        &mut self, _provider_id: i64, config: HashMap<String, String>,
        http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let base_url = config::get_base_url(&config)?;
        let token = required_value(&config, "token", "Missing Gitea access token")?.to_string();
        let status_source = custom_status_source(&config)?;

        let http_client = require_client(http_client)?;

        self.status_source = status_source;
        self.client = Some(client::GiteaClient::new(http_client, base_url, token));
        self.config = config;
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        self.client()?.get_user().await?;
        Ok(true)
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        self.client()?.list_organizations().await
    }

    async fn ids_with_ci(&self, ids: Vec<String>) -> PluginResult<Vec<String>> {
        let client = self.client()?;
        Ok(retain_with_ci(ids, |id| async move {
            let (owner, repo) = config::parse_repo(&id)?;
            Ok(!client.list_workflows(owner, repo).await?.is_empty())
        })
        .await)
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        let params = params.unwrap_or_default();
        let (uid, exclusive) = match org {
            Some(org) => (
                org.parse().map_err(|_| {
                    PluginError::InvalidConfig(format!("Invalid organization ID: {org}"))
                })?,
                true,
            ),
            None => (client.get_user().await?.id, false),
        };

        let response = client
            .search_repositories(uid, exclusive, search.as_deref(), &params)
            .await?;

        Ok(PaginatedResponse {
            items: response
                .items
                .iter()
                .map(mapper::map_available_pipeline)
                .collect(),
            page: response.page,
            page_size: response.page_size,
            total_count: response.total_count,
            total_pages: response.total_pages,
            has_more: response.has_more,
        })
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let id = config::parse_pipeline_id(pipeline_id)?;
        let runs = self
            .client()?
            .workflow_runs(id.owner, id.repo, id.workflow, limit)
            .await?;
        Ok(runs
            .iter()
            .map(|run| mapper::map_run(run, pipeline_id, id.owner))
            .collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let id = config::parse_pipeline_id(pipeline_id)?;
        let run = self
            .client()?
            .find_run(id.owner, id.repo, id.workflow, run_number)
            .await?;
        Ok(mapper::map_run(&run, pipeline_id, id.owner))
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let id = config::parse_pipeline_id(run.pipeline_id)?;
        let run_id = run
            .metadata
            .get("run_id")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| {
                PluginError::InvalidConfig(format!("Run #{} has no Gitea run ID", run.run_number))
            })?;

        let jobs = self
            .client()?
            .list_run_jobs(id.owner, id.repo, run_id)
            .await?;

        Ok(mapper::jobs_to_log_nodes(&jobs))
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let id = config::parse_pipeline_id(run.pipeline_id)?;
        let node_id = digit_id(node_id, "Gitea job ID")?;

        let client = self.client()?;
        let attempt = client.job_attempt(id.owner, id.repo, node_id).await?;
        let position = resume_position::<LogPosition>(from, sink, |position| {
            position.job == node_id && position.attempt == attempt
        })?;
        let (mode, offset) = client
            .stream_job_log(id.owner, id.repo, node_id, position.map(|p| p.offset), sink)
            .await?;
        let job = node_id.to_string();
        Ok(LogRead {
            mode,
            cursor: Some(LogCursor::new(&LogPosition {
                job,
                attempt,
                offset,
            })?),
        })
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &GiteaLogDialect
    }

    async fn fetch_workflow_parameters(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        let id = config::parse_pipeline_id(workflow_id)?;
        let client = self.client()?;
        let workflows = client.list_workflows(id.owner, id.repo).await?;
        let path = workflows
            .iter()
            .find(|workflow| workflow.id == id.workflow)
            .map(|workflow| workflow.path.as_str())
            .ok_or_else(|| {
                PluginError::PipelineNotFound(format!("Workflow {} not found", id.workflow))
            })?;
        let source = client.raw_file(id.owner, id.repo, path).await?;

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

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<TriggerResult> {
        let id = config::parse_pipeline_id(&params.workflow_id)?;
        let client = self.client()?;

        let body = dispatch_body(params.inputs);

        client
            .dispatch_workflow(id.owner, id.repo, id.workflow, &body)
            .await?;
        Ok(TriggerResult::default())
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let id = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        let run = client
            .find_run(id.owner, id.repo, id.workflow, run_number)
            .await?;
        client.cancel_run(id.owner, id.repo, run.id).await
    }
});

#[cfg(test)]
#[cfg(not(target_os = "ios"))]
mod tests;
