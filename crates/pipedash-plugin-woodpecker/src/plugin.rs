use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine;
use pipedash_plugin_api::config::{
    required_value,
    selected_items,
};
use pipedash_plugin_api::pagination::paginate;
use pipedash_plugin_api::support::{
    digit_id,
    organizations,
    require_configured,
    resume_position,
    try_buffered,
    write_in_chunks,
    CONCURRENCY,
};
use pipedash_plugin_api::*;

use crate::log_dialect::WoodpeckerLogDialect;
use crate::mapper::BRANCH_INPUT;
use crate::{
    client,
    config,
    mapper,
    metadata,
    types,
};

#[derive(serde::Serialize, serde::Deserialize)]
struct LogPosition {
    step: String,
    lines: usize,
}

plugin_struct!(
    WoodpeckerPlugin,
    client::WoodpeckerClient,
    metadata::create_metadata();
    status_source: Option<StatusSource> = None
);

impl WoodpeckerPlugin {
    async fn fetch_repo_pipeline(&self, full_name: &str) -> PluginResult<Pipeline> {
        let client = self.client()?;
        let repo = client.lookup_repo(full_name).await?;
        let latest = client.list_pipelines(repo.id, 1).await?;
        Ok(mapper::map_pipeline(&repo, latest.first()))
    }

    async fn write_entries(entries: &[types::LogEntry], sink: &mut LogSink) -> PluginResult<()> {
        write_in_chunks(sink, entries, |chunk, entry| {
            if let Some(data) = &entry.data {
                base64::engine::general_purpose::STANDARD
                    .decode_vec(data, chunk)
                    .map_err(|e| {
                        PluginError::SerializationError(format!("Invalid Woodpecker log line: {e}"))
                    })?;
            }
            chunk.push(b'\n');
            Ok(())
        })
        .await
    }
}

plugin_impl!(WoodpeckerPlugin [metadata, status_source, available_from_filtered] {
    fn initialize(
        &mut self, _provider_id: i64, config: HashMap<String, String>,
        http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let base_url = config::get_base_url(&config)?;
        let token = required_value(&config, "token", "Missing Woodpecker personal access token")?
            .to_string();
        let status_source = custom_status_source(&config)?;

        let http_client = require_client(http_client)?;

        self.status_source = status_source;
        self.client = Some(client::WoodpeckerClient::new(http_client, base_url, token));
        self.config = config;
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        self.client()?.ping().await?;
        Ok(true)
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        let repos = self.client()?.list_repos().await?;
        Ok(organizations(repos.iter().map(|repo| repo.owner.as_str()), None))
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedAvailablePipelines> {
        let client = self.client()?;

        let term = search.as_deref().map(str::to_lowercase);
        let mut repos = client.list_repos().await?;
        repos.retain(|repo| {
            org.as_deref().is_none_or(|org| repo.owner == org)
                && term
                    .as_deref()
                    .is_none_or(|term| repo.full_name.to_lowercase().contains(term))
        });

        let items = repos
            .iter()
            .map(mapper::map_available_pipeline)
            .collect::<Vec<_>>();
        paginate(items, params.unwrap_or_default())
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let repositories = require_configured(
            selected_items(&self.config).unwrap_or_default(),
            "repositories",
        )?;

        let requests: Vec<_> = repositories
            .iter()
            .map(|full_name| self.fetch_repo_pipeline(full_name))
            .collect();
        try_buffered(requests, CONCURRENCY).await
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let repo_id = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        let source = mapper::source_repo(&client.get_repo(repo_id).await?);
        let runs = client.list_pipelines(repo_id, limit).await?;
        Ok(runs
            .iter()
            .map(|run| {
                mapper::map_run(
                    run,
                    pipeline_id,
                    config::pipeline_web_url(client.base_url(), repo_id, run.number),
                    source.as_ref(),
                )
            })
            .collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let repo_id = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        let source = mapper::source_repo(&client.get_repo(repo_id).await?);
        let run = client.get_pipeline(repo_id, run_number).await?;
        Ok(mapper::map_run(
            &run,
            pipeline_id,
            config::pipeline_web_url(client.base_url(), repo_id, run.number),
            source.as_ref(),
        ))
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let repo_id = config::parse_pipeline_id(run.pipeline_id)?;
        let client = self.client()?;
        let pipeline = client.get_pipeline(repo_id, run.run_number).await?;
        Ok(mapper::workflows_to_log_nodes(
            &pipeline,
            &config::pipeline_web_url(client.base_url(), repo_id, run.run_number),
        ))
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let node_id = digit_id(node_id, "Woodpecker step ID")?;
        let repo_id = config::parse_pipeline_id(run.pipeline_id)?;
        let entries = self
            .client()?
            .step_logs(repo_id, run.run_number, node_id)
            .await?;

        let resumed = resume_position::<LogPosition>(from, sink, |position| {
            position.step == node_id && position.lines <= entries.len()
        })?;
        Self::write_entries(&entries[resumed.as_ref().map_or(0, |p| p.lines)..], sink).await?;

        let cursor = LogCursor::new(&LogPosition {
            step: node_id.to_string(),
            lines: entries.len(),
        })?;
        Ok(match resumed {
            Some(_) => LogRead::append(cursor),
            None => LogRead::replace(Some(cursor)),
        })
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &WoodpeckerLogDialect
    }

    async fn fetch_workflow_parameters(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        let repo_id = config::parse_pipeline_id(workflow_id)?;
        let repo = self.client()?.get_repo(repo_id).await?;
        Ok(vec![WorkflowParameter {
            name: BRANCH_INPUT.to_string(),
            label: Some("Branch".to_string()),
            description: Some("Branch to run the pipeline on".to_string()),
            param_type: WorkflowParameterType::String {
                default: Some(repo.default_branch).filter(|branch| !branch.is_empty()),
            },
            required: true,
        }])
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<TriggerResult> {
        let repo_id = config::parse_pipeline_id(&params.workflow_id)?;
        let branch = params
            .inputs
            .as_ref()
            .and_then(|inputs| inputs.get(BRANCH_INPUT))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|branch| !branch.is_empty())
            .ok_or_else(|| {
                PluginError::InvalidConfig(
                    "Branch is required to start a Woodpecker pipeline".to_string(),
                )
            })?;
        let client = self.client()?;
        let pipeline = client
            .create_pipeline(repo_id, branch)
            .await?
            .ok_or_else(|| {
                PluginError::ApiError(
                    "Woodpecker started no pipeline: no workflow runs on manual events".to_string(),
                )
            })?;
        Ok(TriggerResult {
            run_id: None,
            run_number: Some(pipeline.number),
        })
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        self.client()?
            .cancel_pipeline(config::parse_pipeline_id(pipeline_id)?, run_number)
            .await
    }
});

#[cfg(test)]
#[cfg(not(target_os = "ios"))]
mod tests;
