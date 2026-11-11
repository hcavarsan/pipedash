use std::collections::HashMap;

use pipedash_plugin_api::support::{
    digit_id,
    input_str,
    resume_position,
    string_parameter,
};
use pipedash_plugin_api::*;

use self::catalog::{
    available_page,
    namespace_of,
};
use crate::log_dialect::GitlabLogDialect;
use crate::{
    client,
    config,
    mapper,
    metadata,
    types,
};

mod catalog;

const DETAIL_DEPLOYMENTS: usize = 100;

#[derive(serde::Serialize, serde::Deserialize)]
struct LogPosition {
    job: String,
    offset: u64,
}

plugin_struct!(
    GitLabPlugin,
    client::GitLabClient,
    metadata::create_metadata();
    status_source: Option<StatusSource> = None
);

plugin_impl!(GitLabPlugin [
    metadata,
    status_source,
    fetch_pipelines,
    fetch_run_history,
] {
    fn initialize(
        &mut self, _provider_id: i64, config: HashMap<String, String>,
        http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let token = config
            .get("token")
            .ok_or_else(|| PluginError::InvalidConfig("Missing GitLab access token".to_string()))?
            .to_string();

        let status_source = config::status_source(&config)?;
        let base_url = config::get_base_url(&config);
        let api_url = config::build_api_url(&base_url);

        let client = require_client(http_client)?;

        self.status_source = status_source;
        self.client = Some(client::GitLabClient::new(client, api_url, token));
        self.config = config;

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client()?;
        client.get_user().await?;
        Ok(true)
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let params = params.unwrap_or_default();
        let response = self.fetch_projects_paginated(&params).await?;

        Ok(available_page(response))
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<pipedash_plugin_api::Organization>> {
        let client = self.client()?;
        client.list_groups().await
    }

    async fn ids_with_ci(&self, ids: Vec<String>) -> PluginResult<Vec<String>> {
        let client = self.client()?;
        let projects: HashMap<String, types::Project> = self
            .fetch_all_projects()
            .await?
            .into_iter()
            .map(|project| (project.name_with_namespace.replace(' ', ""), project))
            .collect();
        Ok(retain_with_ci(ids, |id| {
            let project = projects.get(&id).cloned();
            async move {
                let Some(project) = project else {
                    return Ok(true);
                };
                if !client
                    .get_project_pipelines(project.id, 1)
                    .await?
                    .is_empty()
                {
                    return Ok(true);
                }
                client.has_ci_config(&project).await
            }
        })
        .await)
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let params = params.unwrap_or_default();
        let client = self.client()?;
        let response = client.list_projects_filtered(org, search, &params).await?;

        Ok(available_page(response))
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let project_id = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;

        let project = client.get_project(project_id).await?;
        let pipeline = client.get_pipeline(project_id, run_number).await?;

        let namespace = namespace_of(&project);
        let mut targets = self.run_targets(&project, DETAIL_DEPLOYMENTS).await?;

        Ok(mapper::map_pipeline_run(
            &pipeline,
            &project,
            namespace.as_deref(),
            targets.remove(&pipeline.id).unwrap_or_default(),
        ))
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let project_id = config::parse_pipeline_id(run.pipeline_id)?;
        let jobs = self
            .client()?
            .get_pipeline_jobs(project_id, run.run_number)
            .await?;

        Ok(mapper::jobs_to_log_nodes(&jobs))
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let node_id = digit_id(node_id, "GitLab job ID")?;
        let project_id = config::parse_pipeline_id(run.pipeline_id)?;

        let position =
            resume_position::<LogPosition>(from, sink, |position| position.job == node_id)?;
        let (mode, offset) = self
            .client()?
            .stream_job_log(project_id, node_id, position.map(|p| p.offset), sink)
            .await?;
        let job = node_id.to_string();
        Ok(LogRead {
            mode,
            cursor: Some(LogCursor::new(&LogPosition { job, offset })?),
        })
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &GitlabLogDialect
    }

    async fn fetch_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        Ok(vec![string_parameter(
            "ref",
            "Ref",
            "Branch, tag, or commit SHA to run pipeline on",
            Some("main".to_string()),
            true,
        )])
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<TriggerResult> {
        let project_id = config::parse_pipeline_id(&params.workflow_id)?;

        let client = self.client()?;

        let ref_name = input_str(&params, "ref").unwrap_or("main").to_string();

        let variables = params.inputs.as_ref().and_then(|inputs| {
            inputs.get("variables").and_then(|vars| {
                vars.as_object().map(|obj| {
                    obj.iter()
                        .map(|(key, value)| types::PipelineVariable {
                            key: key.clone(),
                            value: value.as_str().unwrap_or("").to_string(),
                            variable_type: Some("env_var".to_string()),
                        })
                        .collect()
                })
            })
        });

        client
            .trigger_pipeline(project_id, ref_name, variables)
            .await?;
        Ok(TriggerResult::default())
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let project_id = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        client.cancel_pipeline(project_id, run_number).await?;
        Ok(())
    }

    fn source_hosts(&self, config: &HashMap<String, String>) -> Vec<String> {
        config::source_hosts(config)
    }

    async fn resolve_commit(&self, repo: &RepoId, rev: &str) -> PluginResult<CommitInfo> {
        mapper::commits::commit_info(self.client()?.get_commit(repo, rev).await?)
    }

    async fn compare_commits(
        &self, repo: &RepoId, base: &CommitSha, head: &CommitSha,
    ) -> PluginResult<CommitComparison> {
        let client = self.client()?;
        let ahead = client.commits_between(repo, base, head).await?;
        let behind = client.commits_between(repo, head, base).await?;
        Ok(mapper::commits::comparison(&ahead, &behind))
    }

    async fn commit_changes(
        &self, repo: &RepoId, sha: &CommitSha,
    ) -> PluginResult<Vec<ChangedFile>> {
        let files = self.client()?.commit_diff(repo, sha).await?;
        Ok(files.into_iter().map(mapper::commits::changed_file).collect())
    }

    async fn registry_auth(&self, host: &str) -> PluginResult<Option<RegistryAuth>> {
        let client = self.client()?;
        let serves_host = config::registry_host(&self.config)
            .is_some_and(|registry| registry.eq_ignore_ascii_case(host));
        let Some(secret) = self.config.get("token").filter(|_| serves_host) else {
            return Ok(None);
        };
        let user = client.get_user().await?;
        Ok(Some(RegistryAuth {
            username: Some(user.username),
            secret: secret.clone(),
            bearer: false,
            token_hosts: config::source_host(&self.config).into_iter().collect(),
        }))
    }

});

#[cfg(test)]
mod tests;
