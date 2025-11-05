//! Buildkite plugin implementation

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;
use pipedash_plugin_api::*;
use reqwest::header::{
    HeaderMap,
    HeaderValue,
    AUTHORIZATION,
};

use crate::{
    client,
    config,
    mapper,
    metadata,
};

/// Buildkite plugin for monitoring builds, agents, and artifacts
pub struct BuildkitePlugin {
    metadata: PluginMetadata,
    client: Option<client::BuildkiteClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
}

impl Default for BuildkitePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildkitePlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            client: None,
            provider_id: None,
            config: HashMap::new(),
        }
    }

    fn client(&self) -> PluginResult<&client::BuildkiteClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }
}

#[async_trait]
impl Plugin for BuildkitePlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
    ) -> PluginResult<()> {
        let token = config
            .get("token")
            .ok_or_else(|| PluginError::InvalidConfig("Missing Buildkite API token".to_string()))?;

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|e| PluginError::InvalidConfig(format!("Invalid token format: {e}")))?,
        );

        let http_client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| PluginError::Internal(format!("Failed to build HTTP client: {e}")))?;

        self.client = Some(client::BuildkiteClient::new(http_client));
        self.provider_id = Some(provider_id);
        self.config = config;

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client()?;

        // Validate by trying to list organizations the user has access to
        let organizations = client.fetch_organizations().await?;

        if organizations.is_empty() {
            Err(PluginError::AuthenticationFailed(
                "No organizations accessible with this token".to_string(),
            ))
        } else {
            Ok(true)
        }
    }

    async fn fetch_available_pipelines(&self) -> PluginResult<Vec<AvailablePipeline>> {
        let client = self.client()?;
        client::fetch_all_available_pipelines(client).await
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self
            .provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))?;

        let (org, pipeline_slugs) = config::parse_selected_items(&self.config)?;

        if pipeline_slugs.is_empty() {
            return Err(PluginError::InvalidConfig(
                "No pipelines configured".to_string(),
            ));
        }

        let client = self.client()?;
        let futures = pipeline_slugs
            .into_iter()
            .map(|slug| client.fetch_pipeline(provider_id, org.clone(), slug));

        let results = join_all(futures).await;

        let mut all_pipelines = Vec::new();
        let mut errors = Vec::new();

        for result in results {
            match result {
                Ok(pipeline) => all_pipelines.push(pipeline),
                Err(e) => errors.push(e),
            }
        }

        if !errors.is_empty() && all_pipelines.is_empty() {
            return Err(errors.into_iter().next().unwrap());
        }

        Ok(all_pipelines)
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 4 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {pipeline_id}"
            )));
        }

        let org = parts[2];
        let slug = parts[3];

        let client = self.client()?;
        let builds = client.fetch_builds(org, slug, limit).await?;

        let pipeline_runs = builds
            .into_iter()
            .map(|build| client::build_to_pipeline_run(build, pipeline_id))
            .collect();

        Ok(pipeline_runs)
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 4 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {pipeline_id}"
            )));
        }

        let org = parts[2];
        let slug = parts[3];

        let client = self.client()?;
        let builds = client.fetch_builds(org, slug, 100).await?;

        let build = builds
            .into_iter()
            .find(|b| b.number == run_number)
            .ok_or_else(|| {
                PluginError::PipelineNotFound(format!(
                    "Build #{run_number} not found for pipeline {pipeline_id}"
                ))
            })?;

        Ok(client::build_to_pipeline_run(build, pipeline_id))
    }

    async fn fetch_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        Ok(vec![WorkflowParameter {
            name: "branch".to_string(),
            label: Some("Branch".to_string()),
            description: Some("Branch to build".to_string()),
            param_type: WorkflowParameterType::String {
                default: Some("main".to_string()),
            },
            required: true,
        }])
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let parts: Vec<&str> = params.workflow_id.split("__").collect();
        if parts.len() != 4 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid workflow ID format: {}",
                params.workflow_id
            )));
        }

        let org = parts[2];
        let slug = parts[3];

        let branch = params
            .inputs
            .as_ref()
            .and_then(|inputs| inputs.get("branch"))
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string();

        let client = self.client()?;
        let build = client
            .trigger_build(org, slug, branch.clone(), params.inputs)
            .await?;

        Ok(serde_json::json!({
            "message": format!("Triggered build #{} on branch {}", build.number, branch),
            "build_number": build.number,
            "build_url": build.web_url
        })
        .to_string())
    }

    async fn fetch_agents(&self) -> PluginResult<Vec<BuildAgent>> {
        let (org, _) = config::parse_selected_items(&self.config)?;

        let client = self.client()?;
        let agents = client.fetch_agents(&org).await?;

        Ok(agents.into_iter().map(mapper::map_agent).collect())
    }

    async fn fetch_artifacts(&self, run_id: &str) -> PluginResult<Vec<BuildArtifact>> {
        let (org, _) = config::parse_selected_items(&self.config)?;

        // Extract build ID from run_id (format: "buildkite-build-{build_id}")
        let build_id = run_id
            .strip_prefix("buildkite-build-")
            .ok_or_else(|| PluginError::InvalidConfig(format!("Invalid run ID: {run_id}")))?;

        let client = self.client()?;
        let artifacts = client.fetch_artifacts(&org, build_id).await?;

        Ok(artifacts
            .into_iter()
            .map(|artifact| client::artifact_to_build_artifact(artifact, run_id))
            .collect())
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 4 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {pipeline_id}"
            )));
        }

        let org = parts[2];
        let slug = parts[3];

        let client = self.client()?;
        client.cancel_build(org, slug, run_number).await
    }
}
