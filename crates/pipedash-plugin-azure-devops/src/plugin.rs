use std::collections::{
    HashMap,
    HashSet,
};
use std::sync::Arc;

use pipedash_plugin_api::config::{
    required_value,
    selected_items,
};
use pipedash_plugin_api::pagination::paginate;
use pipedash_plugin_api::support::{
    organizations,
    require_configured,
    try_buffered,
    uuid_id,
    CONCURRENCY,
};
use pipedash_plugin_api::*;

use crate::log_dialect::AzureDevOpsLogDialect;
use crate::{
    client,
    config,
    mapper,
    metadata,
    types,
};

mod commits;

const SOURCE_HOST: &str = "dev.azure.com";

plugin_struct!(
    AzureDevOpsPlugin,
    client::AzureDevOpsClient,
    metadata::create_metadata();
    organization: String = String::new(),
    status_source: Option<StatusSource> = None
);

impl AzureDevOpsPlugin {
    async fn fetch_project_pipelines(&self, project: &str) -> PluginResult<Vec<Pipeline>> {
        let definitions = self.client()?.list_definitions(project).await?;
        Ok(definitions
            .iter()
            .map(|definition| mapper::map_pipeline(&self.organization, project, definition))
            .collect())
    }

    async fn owned_build(
        &self, project: &str, definition_id: i64, pipeline_id: &str, build_id: i64,
    ) -> PluginResult<types::Build> {
        let build = self.client()?.get_build(project, build_id).await?;
        let belongs_to_pipeline = build
            .definition
            .as_ref()
            .and_then(|definition| definition.id)
            .is_none_or(|id| id == definition_id);
        if !belongs_to_pipeline {
            return Err(PluginError::PipelineNotFound(format!(
                "Run #{build_id} does not belong to {pipeline_id}"
            )));
        }
        Ok(build)
    }
}

plugin_impl!(AzureDevOpsPlugin [
    metadata,
    status_source,
    available_from_filtered,
] {
    fn initialize(
        &mut self, _provider_id: i64, config: HashMap<String, String>,
        http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let server_url = config::get_server_url(&config)?;
        let token = required_value(
            &config,
            "token",
            "Missing Azure DevOps personal access token",
        )?
        .to_string();
        let status_source = custom_status_source(&config)?;

        let http_client = require_client(http_client)?;

        self.organization = config::organization_of(&server_url);
        self.status_source = status_source;
        self.client = Some(client::AzureDevOpsClient::new(
            http_client,
            server_url,
            token,
        ));
        self.config = config;
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        self.client()?.ping().await?;
        Ok(true)
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        self.client()?;
        Ok(organizations([self.organization.as_str()], None))
    }

    async fn ids_with_ci(&self, ids: Vec<String>) -> PluginResult<Vec<String>> {
        let client = self.client()?;
        Ok(retain_with_ci(ids, |project| async move {
            client.has_definitions(&project).await
        })
        .await)
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedAvailablePipelines> {
        let client = self.client()?;

        let mut projects = if org.is_none_or(|org| org == self.organization) {
            client.list_projects().await?
        } else {
            Vec::new()
        };
        if let Some(term) = search.as_deref().map(str::to_lowercase) {
            projects.retain(|project| {
                project.name.to_lowercase().contains(&term)
                    || project
                        .description
                        .as_deref()
                        .is_some_and(|description| description.to_lowercase().contains(&term))
            });
        }
        projects.sort_by_cached_key(|project| project.name.to_lowercase());

        let items = projects
            .iter()
            .map(|project| mapper::map_available_pipeline(project, &self.organization))
            .collect::<Vec<_>>();
        paginate(items, params.unwrap_or_default())
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let projects = require_configured(
            selected_items(&self.config).unwrap_or_default(),
            "projects",
        )?;
        let requests: Vec<_> = projects
            .iter()
            .map(|project| self.fetch_project_pipelines(project))
            .collect();
        let results = try_buffered(requests, CONCURRENCY).await?;

        Ok(results.into_iter().flatten().collect())
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let id = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        let builds = client
            .list_builds(id.project, id.definition_id, limit)
            .await?;
        Ok(builds
            .iter()
            .map(|build| mapper::map_run(build, pipeline_id, id.project, client.server_url()))
            .collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let id = config::parse_pipeline_id(pipeline_id)?;
        let build = self
            .owned_build(id.project, id.definition_id, pipeline_id, run_number)
            .await?;
        Ok(mapper::map_run(
            &build,
            pipeline_id,
            id.project,
            self.client()?.server_url(),
        ))
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let id = config::parse_pipeline_id(run.pipeline_id)?;
        let timeline = self
            .client()?
            .get_timeline(id.project, run.run_number)
            .await?;
        Ok(mapper::timeline_to_log_nodes(&timeline))
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let id = config::parse_pipeline_id(run.pipeline_id)?;
        let record_id = uuid_id(node_id, "Azure DevOps log node ID")?;
        let client = self.client()?;
        let timeline = client.get_timeline(id.project, run.run_number).await?;
        let (log_id, attempt) = mapper::job_log_ref(&timeline, record_id).ok_or_else(|| {
            PluginError::PipelineNotFound(format!(
                "Run #{} has no job log {node_id}",
                run.run_number
            ))
        })?;
        client
            .stream_build_log(id.project, run.run_number, log_id, attempt, from, sink)
            .await
    }

    fn source_hosts(&self, config: &HashMap<String, String>) -> Vec<String> {
        if config.is_empty() {
            Vec::new()
        } else {
            vec![SOURCE_HOST.to_string()]
        }
    }

    async fn resolve_commit(&self, repo: &RepoId, rev: &str) -> PluginResult<CommitInfo> {
        self.resolve_commit_impl(repo, rev).await
    }

    async fn compare_commits(
        &self, repo: &RepoId, base: &CommitSha, head: &CommitSha,
    ) -> PluginResult<CommitComparison> {
        self.compare_commits_impl(repo, base, head).await
    }

    async fn commit_changes(
        &self, repo: &RepoId, sha: &CommitSha,
    ) -> PluginResult<Vec<ChangedFile>> {
        self.commit_changes_impl(repo, sha).await
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &AzureDevOpsLogDialect
    }

    async fn fetch_workflow_parameters(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        let id = config::parse_pipeline_id(workflow_id)?;
        let definition = self
            .client()?
            .get_definition(id.project, id.definition_id)
            .await?;

        let default_branch = definition
            .repository
            .and_then(|repository| repository.default_branch)
            .filter(|branch| !branch.is_empty())
            .map(|branch| mapper::short_branch(&branch).to_string());
        let mut parameters = vec![WorkflowParameter {
            name: mapper::BRANCH_INPUT.to_string(),
            label: Some("Branch".to_string()),
            description: Some(
                "Branch to build. The definition's default branch is used when empty".to_string(),
            ),
            param_type: WorkflowParameterType::String {
                default: default_branch,
            },
            required: false,
        }];
        parameters.extend(
            definition
                .variables
                .unwrap_or_default()
                .into_iter()
                .filter(|(_, variable)| variable.is_overridable())
                .map(|(name, variable)| WorkflowParameter {
                    label: Some(name.clone()),
                    description: Some("Pipeline variable".to_string()),
                    param_type: WorkflowParameterType::String {
                        default: variable.value,
                    },
                    required: false,
                    name,
                }),
        );
        Ok(parameters)
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<TriggerResult> {
        let id = config::parse_pipeline_id(&params.workflow_id)?;
        let client = self.client()?;
        let overridable: HashSet<String> = client
            .get_definition(id.project, id.definition_id)
            .await?
            .variables
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, variable)| variable.is_overridable())
            .map(|(name, _)| name.to_lowercase())
            .collect();
        let body = mapper::trigger_body(id.definition_id, params.inputs, &overridable);
        let build = client.queue_build(id.project, &body).await?;
        Ok(TriggerResult {
            run_id: None,
            run_number: Some(build.id),
        })
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let id = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        self.owned_build(id.project, id.definition_id, pipeline_id, run_number)
            .await?;
        client.cancel_build(id.project, run_number).await
    }
});

#[cfg(test)]
#[cfg(not(target_os = "ios"))]
mod tests;
