use std::collections::HashMap;

use octocrab::service::middleware::retry::RetryConfig;
use octocrab::Octocrab;
use pipedash_plugin_api::*;

use crate::{
    client,
    config,
    metadata,
};

mod actions;
mod catalog;
mod hooks;
mod logs;
mod permissions;

plugin_struct!(
    GitHubPlugin,
    client::GitHubClient,
    metadata::create_metadata();
    status_source: Option<StatusSource> = None
);

plugin_impl!(GitHubPlugin [
    metadata,
    status_source,
    fetch_pipelines,
    fetch_run_history,
    fetch_run_details,
    fetch_log_nodes,
    read_log,
    fetch_workflow_parameters,
    trigger_pipeline,
    cancel_run,
    force_cancel_run,
    review_run,
    rerun_failed_jobs,
    rerun_run,
    enable_pipeline,
    get_feature_availability,
] {
    fn pipeline_actions(&self, pipeline: &PipelineFacts<'_>) -> PipelineActionState {
        let mut state = pipedash_plugin_api::plugin::defaults::pipeline_actions(
            &self.metadata().capabilities,
            pipeline,
        );
        state.rerun_strategy = RerunStrategy::InPlace;
        if pipeline
            .workflow_file
            .is_some_and(|file| file.starts_with(config::DYNAMIC_WORKFLOW_PREFIX))
        {
            state.trigger = ActionAvailability::blocked(
                "GitHub starts this built-in workflow itself. It has no workflow file, so only its existing runs can be rerun.",
            );
        }
        state
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let token = config
            .get("token")
            .ok_or_else(|| PluginError::InvalidConfig("Missing GitHub token".to_string()))?;

        if token.is_empty() {
            tracing::error!(provider_id = provider_id, "GitHub token is empty");
            return Err(PluginError::InvalidConfig(
                "GitHub token is empty. Please check keyring permissions.".to_string(),
            ));
        }

        tracing::debug!(token_length = token.len(), "Initializing GitHub plugin");

        let base_url = config::get_base_url(&config);
        let api_url = config::build_api_url(&base_url)?;
        let status_source = config::status_source(&config, &base_url)?;

        tracing::debug!(api_url = %api_url, "Using GitHub API URL");

        let http_client = (*require_client(http_client)?).clone();

        let octocrab = Octocrab::builder()
            .add_retry_config(RetryConfig::None)
            .personal_token(token.clone())
            .base_uri(&api_url)
            .map_err(|e| PluginError::InvalidConfig(format!("Failed to set base URI: {e}")))?
            .build_with_reqwest(http_client.clone())
            .map_err(|e| {
                PluginError::InvalidConfig(format!("Failed to build GitHub client: {e}"))
            })?;

        let github_client =
            client::GitHubClient::new(octocrab, http_client, token.clone(), &api_url)?;
        self.client = Some(github_client);
        self.config = config;
        self.status_source = status_source;

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client()?;

        client
            .retry_policy
            .retry(|| async {
                client
                    .octocrab
                    .current()
                    .user()
                    .await
                    .map(|_| true)
                    .map_err(|e| client::map_octocrab_error("Failed to validate credentials", e))
            })
            .await
    }

    async fn fetch_rate_limit(&self) -> PluginResult<Option<RateLimit>> {
        self.client()?.fetch_rate_limit().await.map(Some)
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        client.fetch_all_repositories(params).await
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<pipedash_plugin_api::Organization>> {
        let client = self.client()?;
        client.fetch_organizations().await
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        client
            .fetch_available_pipelines_filtered(org, search, params)
            .await
    }

    async fn ids_with_ci(&self, ids: Vec<String>) -> PluginResult<Vec<String>> {
        let client = self.client()?;
        Ok(retain_with_ci(ids, |id| async move {
            let (owner, repo) = config::parse_repo(&id)
                .ok_or_else(|| PluginError::InvalidConfig(format!("Invalid repository: {id}")))?;
            client.has_workflows(&owner, &repo).await
        })
        .await)
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &pipedash_plugin_actions::ActionsLogDialect
    }

    async fn check_permissions(&self) -> PluginResult<PermissionStatus> {
        let client = self.client()?;
        client
            .check_token_permissions(&config::get_repositories(&self.config))
            .await
    }

    fn source_hosts(&self, config: &HashMap<String, String>) -> Vec<String> {
        config::source_hosts(config)
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

    async fn artifact_provenance(
        &self, _repository: &ArtifactRepository, digest: &Digest,
    ) -> PluginResult<Option<Provenance>> {
        self.artifact_provenance_impl(digest).await
    }

    async fn registry_auth(&self, host: &str) -> PluginResult<Option<RegistryAuth>> {
        self.registry_auth_impl(host).await
    }
});

#[cfg(test)]
mod tests;
