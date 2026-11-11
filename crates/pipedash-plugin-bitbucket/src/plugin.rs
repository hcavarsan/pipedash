use std::collections::HashMap;

use pipedash_plugin_api::support::{
    resume_position,
    string_parameter,
};
use pipedash_plugin_api::*;

use crate::log_dialect::BitbucketLogDialect;
use crate::{
    client,
    config,
    mapper,
    metadata,
    types,
};

mod catalog;

#[derive(serde::Serialize, serde::Deserialize)]
struct LogPosition {
    pipeline: String,
    step: String,
    offset: u64,
}

plugin_struct!(
    BitbucketPlugin,
    client::BitbucketClient,
    metadata::create_metadata();
    status_source: Option<StatusSource> = None
);

plugin_impl!(BitbucketPlugin [
    metadata,
    status_source,
    fetch_organizations,
    ids_with_ci,
    fetch_available_pipelines,
    fetch_available_pipelines_filtered,
    fetch_pipelines,
] {
    fn initialize(
        &mut self, _provider_id: i64, config: HashMap<String, String>,
        http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let (email, api_token) = config::get_auth(&config)?;
        let api_url = config::get_api_url();

        let credentials = format!("{}:{}", email, api_token);
        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            credentials.as_bytes(),
        );
        let auth_value = format!("Basic {}", encoded);

        let client = require_client(http_client)?;

        self.status_source = Some(config::status_source()?);
        self.client = Some(client::BitbucketClient::new(client, api_url, auth_value));
        self.config = config;

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client()?;
        client.get_user().await?;
        Ok(true)
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let (workspace, repo_slug) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;

        let pipelines = client.list_pipelines(&workspace, &repo_slug, limit).await?;

        Ok(pipelines
            .iter()
            .map(|p| mapper::map_pipeline_run(p, &workspace, &repo_slug))
            .collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let (workspace, repo_slug) = config::parse_pipeline_id(pipeline_id)?;

        let pipeline = self
            .client()?
            .find_pipeline_by_build_number(&workspace, &repo_slug, run_number)
            .await?;

        Ok(mapper::map_pipeline_run(&pipeline, &workspace, &repo_slug))
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let (workspace, repo_slug) = config::parse_pipeline_id(run.pipeline_id)?;
        let client = self.client()?;

        let pipeline = client
            .find_pipeline_by_build_number(&workspace, &repo_slug, run.run_number)
            .await?;
        let steps = client
            .list_steps(&workspace, &repo_slug, &pipeline.uuid)
            .await?;

        Ok(mapper::steps_to_log_nodes(&steps))
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let (workspace, repo_slug) = config::parse_pipeline_id(run.pipeline_id)?;
        config::check_step_uuid(node_id)?;
        let client = self.client()?;

        let pipeline = client
            .find_pipeline_by_build_number(&workspace, &repo_slug, run.run_number)
            .await?;

        let position = resume_position::<LogPosition>(from, sink, |position| {
            position.pipeline == pipeline.uuid && position.step == node_id
        })?;
        let (mode, offset) = client
            .stream_step_log(
                &workspace,
                &repo_slug,
                &pipeline.uuid,
                node_id,
                position.map(|p| p.offset),
                sink,
            )
            .await?;
        Ok(LogRead {
            mode,
            cursor: Some(LogCursor::new(&LogPosition {
                pipeline: pipeline.uuid,
                step: node_id.to_string(),
                offset,
            })?),
        })
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &BitbucketLogDialect
    }

    async fn fetch_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        Ok(vec![
            string_parameter(
                "ref",
                "Branch",
                "Branch name to run pipeline on",
                Some("main".to_string()),
                true,
            ),
            string_parameter(
                "custom_pipeline",
                "Custom Pipeline",
                "Name of custom pipeline to run (optional)",
                None,
                false,
            ),
        ])
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<TriggerResult> {
        let (workspace, repo_slug) = config::parse_pipeline_id(&params.workflow_id)?;
        let client = self.client()?;

        let ref_name = params
            .inputs
            .as_ref()
            .and_then(|i| i.get("ref"))
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string();

        let custom_pipeline = params
            .inputs
            .as_ref()
            .and_then(|i| i.get("custom_pipeline"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());

        let request = types::TriggerPipelineRequest {
            target: types::TriggerTarget {
                target_type: "pipeline_ref_target".to_string(),
                ref_name,
                ref_type: "branch".to_string(),
                selector: custom_pipeline.map(|name| types::TriggerSelector {
                    selector_type: "custom".to_string(),
                    pattern: Some(name.to_string()),
                }),
            },
        };

        client
            .trigger_pipeline(&workspace, &repo_slug, request)
            .await?;

        Ok(TriggerResult::default())
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let (workspace, repo_slug) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;

        let pipeline = client
            .find_pipeline_by_build_number(&workspace, &repo_slug, run_number)
            .await?;

        let state = pipeline.state.name.as_str();
        if state == "PAUSED" || state == "HALTED" {
            return Err(PluginError::ApiError(format!(
                "Cannot cancel pipeline in '{}' state. Pipelines waiting for manual approval \
                 must be cancelled directly in Bitbucket UI.",
                state
            )));
        }

        if state == "COMPLETED" {
            return Err(PluginError::ApiError(
                "Cannot cancel a completed pipeline.".to_string(),
            ));
        }

        if state == "IN_PROGRESS" {
            let pipeline_is_paused = pipeline
                .state
                .stage
                .as_ref()
                .map(|s| s.name == "PAUSED")
                .unwrap_or(false);

            if pipeline_is_paused {
                return Err(PluginError::ApiError(
                    "Cannot cancel pipeline waiting for manual approval. \
                     Please cancel directly in Bitbucket UI."
                        .to_string(),
                ));
            }

            let steps = client
                .list_steps(&workspace, &repo_slug, &pipeline.uuid)
                .await?;

            let has_paused_step = steps.iter().any(|step| {
                step.state.name == "PENDING"
                    && step
                        .state
                        .stage
                        .as_ref()
                        .map(|s| s.name == "PAUSED")
                        .unwrap_or(false)
            });

            if has_paused_step {
                return Err(PluginError::ApiError(
                    "Cannot cancel pipeline with steps waiting for manual approval. \
                     Please cancel directly in Bitbucket UI."
                        .to_string(),
                ));
            }
        }

        client
            .stop_pipeline(&workspace, &repo_slug, &pipeline.uuid)
            .await
    }

    fn source_hosts(&self, config: &HashMap<String, String>) -> Vec<String> {
        config::source_hosts(config)
    }

    async fn resolve_commit(&self, repo: &RepoId, rev: &str) -> PluginResult<CommitInfo> {
        let client = self.client()?;
        let (workspace, repo_slug) = config::parse_repo(repo)?;
        mapper::commits::commit_info(&client.get_commit(workspace, repo_slug, rev).await?)
    }

    async fn compare_commits(
        &self, repo: &RepoId, base: &CommitSha, head: &CommitSha,
    ) -> PluginResult<CommitComparison> {
        let client = self.client()?;
        let (workspace, repo_slug) = config::parse_repo(repo)?;
        let ahead = client
            .commits_between(workspace, repo_slug, head, base)
            .await?;
        let behind = client
            .commits_between(workspace, repo_slug, base, head)
            .await?;
        Ok(mapper::commits::comparison(&ahead, &behind))
    }

    async fn commit_changes(
        &self, repo: &RepoId, sha: &CommitSha,
    ) -> PluginResult<Vec<ChangedFile>> {
        let client = self.client()?;
        let (workspace, repo_slug) = config::parse_repo(repo)?;
        let stats = client.commit_diffstat(workspace, repo_slug, sha).await?;
        let diff = client.commit_diff(workspace, repo_slug, sha).await?;
        Ok(mapper::commits::changed_files(&stats, &diff))
    }


});

#[cfg(test)]
mod tests;
