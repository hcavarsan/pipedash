//! HTTP client and API methods for Buildkite

use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use futures::future::join_all;
use pipedash_plugin_api::{
    AvailablePipeline,
    BuildArtifact,
    Pipeline,
    PipelineRun,
    PluginError,
    PluginResult,
};
use reqwest::Client;

use crate::{
    config,
    mapper,
    types,
};

const BASE_URL: &str = "https://api.buildkite.com/v2";

/// Buildkite API client with retry logic
pub(crate) struct BuildkiteClient {
    client: Client,
}

impl BuildkiteClient {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    /// Retries a request operation with exponential backoff
    async fn retry_request<F, Fut, T>(&self, operation: F) -> PluginResult<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = PluginResult<T>>,
    {
        let max_retries = 3;
        let mut delay = Duration::from_millis(100);
        let mut last_error = None;

        for attempt in 0..max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) if attempt < max_retries - 1 => match &e {
                    PluginError::NetworkError(_) | PluginError::ApiError(_) => {
                        last_error = Some(e);
                        tokio::time::sleep(delay).await;
                        delay *= 2;
                        continue;
                    }
                    _ => return Err(e),
                },
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| PluginError::NetworkError("Max retries exceeded".to_string())))
    }

    /// Fetches all organizations the user has access to
    pub async fn fetch_organizations(&self) -> PluginResult<Vec<types::Organization>> {
        let url = format!("{BASE_URL}/organizations");

        let orgs = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to fetch organizations: {e}")))?
            .json()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to parse organizations: {e}")))?;

        Ok(orgs)
    }

    /// Fetches all pipelines for a given organization
    pub async fn fetch_org_pipelines(
        &self, org_slug: String,
    ) -> PluginResult<Vec<AvailablePipeline>> {
        self.retry_request(|| async {
            let url = format!("{BASE_URL}/organizations/{org_slug}/pipelines?per_page=100");

            let pipelines: Vec<types::Pipeline> = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(|e| {
                    PluginError::ApiError(format!("Failed to fetch pipelines for {org_slug}: {e}"))
                })?
                .json()
                .await
                .map_err(|e| {
                    PluginError::ApiError(format!("Failed to parse pipelines for {org_slug}: {e}"))
                })?;

            Ok(pipelines
                .into_iter()
                .map(|pipeline| {
                    let repo_name = config::parse_repository_name(&pipeline.repository);
                    AvailablePipeline {
                        id: format!("{org_slug}/{}", pipeline.name),
                        name: pipeline.name,
                        description: Some(repo_name.clone()),
                        organization: Some(org_slug.clone()),
                        repository: Some(repo_name),
                    }
                })
                .collect())
        })
        .await
    }

    /// Fetches a single pipeline with its latest build
    pub async fn fetch_pipeline(
        &self, provider_id: i64, org: String, slug: String,
    ) -> PluginResult<Pipeline> {
        self.retry_request(|| async {
            let pipeline_url = format!("{BASE_URL}/organizations/{org}/pipelines/{slug}");

            let pipeline: types::Pipeline = self
                .client
                .get(&pipeline_url)
                .send()
                .await
                .map_err(|e| PluginError::ApiError(format!("Failed to fetch pipeline: {e}")))?
                .json()
                .await
                .map_err(|e| PluginError::ApiError(format!("Failed to parse pipeline: {e}")))?;

            let builds_url =
                format!("{BASE_URL}/organizations/{org}/pipelines/{slug}/builds?per_page=1");

            let builds: Vec<types::Build> = self
                .client
                .get(&builds_url)
                .send()
                .await
                .map_err(|e| PluginError::ApiError(format!("Failed to fetch builds: {e}")))?
                .json()
                .await
                .map_err(|e| PluginError::ApiError(format!("Failed to parse builds: {e}")))?;

            let latest_build = builds.first();
            let status = latest_build
                .map(|build| mapper::map_build_state(&build.state))
                .unwrap_or(pipedash_plugin_api::PipelineStatus::Pending);

            let last_run = latest_build.and_then(|build| {
                chrono::DateTime::parse_from_rfc3339(&build.created_at)
                    .ok()
                    .map(|dt| dt.with_timezone(&Utc))
            });

            let mut metadata = HashMap::new();
            metadata.insert(
                "organization_slug".to_string(),
                serde_json::json!(org.clone()),
            );

            Ok(Pipeline {
                id: format!("buildkite__{provider_id}__{org}__{slug}"),
                provider_id,
                provider_type: "buildkite".to_string(),
                name: pipeline.name,
                status,
                last_run,
                last_updated: Utc::now(),
                repository: config::parse_repository_name(&pipeline.repository),
                branch: latest_build.and_then(|b| {
                    if b.branch.is_empty() {
                        None
                    } else {
                        Some(b.branch.clone())
                    }
                }),
                workflow_file: None,
                metadata,
            })
        })
        .await
    }

    /// Fetches build history for a pipeline
    pub async fn fetch_builds(
        &self, org: &str, slug: &str, limit: usize,
    ) -> PluginResult<Vec<types::Build>> {
        let per_page = 100;
        let total_pages = limit.div_ceil(100);
        let mut all_builds = Vec::new();

        for page in 1..=total_pages.min(10) {
            let url = format!(
                "{BASE_URL}/organizations/{org}/pipelines/{slug}/builds?per_page={}&page={}",
                per_page, page
            );

            let builds: Vec<types::Build> = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(|e| PluginError::ApiError(format!("Failed to fetch builds: {e}")))?
                .json()
                .await
                .map_err(|e| PluginError::ApiError(format!("Failed to parse builds: {e}")))?;

            if builds.is_empty() {
                break;
            }

            all_builds.extend(builds);

            if all_builds.len() >= limit {
                break;
            }
        }

        all_builds.truncate(limit);
        Ok(all_builds)
    }

    /// Triggers a new build for a pipeline
    pub async fn trigger_build(
        &self, org: &str, slug: &str, branch: String, inputs: Option<serde_json::Value>,
    ) -> PluginResult<types::Build> {
        let url = format!("{BASE_URL}/organizations/{org}/pipelines/{slug}/builds");

        let mut body = serde_json::json!({
            "branch": branch,
            "commit": "HEAD",
        });

        if let Some(inputs) = inputs {
            if let Some(obj) = inputs.as_object() {
                if let Some(message) = obj.get("message") {
                    body["message"] = message.clone();
                }
                if let Some(env) = obj.get("env") {
                    body["env"] = env.clone();
                }
            }
        }

        let response = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to trigger build: {e}")))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(PluginError::ApiError(format!(
                "Failed to trigger build: {error_text}"
            )));
        }

        let build = response
            .json()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to parse response: {e}")))?;

        Ok(build)
    }

    /// Fetches agents for an organization
    pub async fn fetch_agents(&self, org: &str) -> PluginResult<Vec<types::Agent>> {
        let url = format!("{BASE_URL}/organizations/{org}/agents");

        let agents = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to fetch agents: {e}")))?
            .json()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to parse agents: {e}")))?;

        Ok(agents)
    }

    /// Fetches artifacts for a build
    pub async fn fetch_artifacts(
        &self, org: &str, build_id: &str,
    ) -> PluginResult<Vec<types::Artifact>> {
        let url = format!("{BASE_URL}/organizations/{org}/builds/{build_id}/artifacts");

        let artifacts = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to fetch artifacts: {e}")))?
            .json()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to parse artifacts: {e}")))?;

        Ok(artifacts)
    }

    /// Cancels a running build
    pub async fn cancel_build(
        &self, org: &str, pipeline_slug: &str, build_number: i64,
    ) -> PluginResult<()> {
        let url = format!(
            "{BASE_URL}/organizations/{org}/pipelines/{pipeline_slug}/builds/{build_number}/cancel"
        );

        eprintln!(
            "[BUILDKITE] Cancelling build #{build_number} for pipeline {org}/{pipeline_slug}"
        );

        let response = self
            .client
            .put(&url)
            .send()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to cancel build: {e}")))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(PluginError::ApiError(format!(
                "Failed to cancel build: {error_text}"
            )));
        }

        eprintln!("[BUILDKITE] Build #{build_number} cancelled successfully");
        Ok(())
    }
}

/// Converts Buildkite Build to PipelineRun
pub(crate) fn build_to_pipeline_run(build: types::Build, pipeline_id: &str) -> PipelineRun {
    let status = mapper::map_build_state(&build.state);

    let started_at = build
        .started_at
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .or_else(|| chrono::DateTime::parse_from_rfc3339(&build.created_at).ok())
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(Utc::now);

    let concluded_at = build
        .finished_at
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let duration_seconds = if let (Some(start), Some(finish)) =
        (build.started_at.as_ref(), build.finished_at.as_ref())
    {
        let start_dt = chrono::DateTime::parse_from_rfc3339(start).ok();
        let finish_dt = chrono::DateTime::parse_from_rfc3339(finish).ok();
        if let (Some(s), Some(f)) = (start_dt, finish_dt) {
            Some((f - s).num_seconds())
        } else {
            None
        }
    } else {
        None
    };

    // Extract inputs for replay functionality
    // For Buildkite, include branch and commit as inputs for replay
    let mut inputs_map = serde_json::Map::new();

    let branch_value = if build.branch.is_empty() {
        "unknown".to_string()
    } else {
        build.branch.clone()
    };

    inputs_map.insert(
        "branch".to_string(),
        serde_json::Value::String(branch_value.clone()),
    );
    inputs_map.insert(
        "commit".to_string(),
        serde_json::Value::String(build.commit.clone()),
    );

    eprintln!(
        "[BUILDKITE] Build #{}: branch={}, commit={}",
        build.number, branch_value, build.commit
    );

    let inputs = Some(serde_json::Value::Object(inputs_map));

    PipelineRun {
        id: format!("buildkite-build-{}", build.id),
        pipeline_id: pipeline_id.to_string(),
        run_number: build.number,
        status,
        started_at,
        concluded_at,
        duration_seconds,
        logs_url: build.web_url.clone(),
        commit_sha: if build.commit.is_empty() {
            None
        } else {
            Some(build.commit.clone())
        },
        commit_message: build.message.clone(),
        branch: Some(branch_value),
        actor: build.author.as_ref().map(|a| a.name.clone()),
        inputs,
        metadata: HashMap::new(), // No additional metadata for runs yet
    }
}

/// Converts Buildkite Artifact to BuildArtifact
pub(crate) fn artifact_to_build_artifact(artifact: types::Artifact, run_id: &str) -> BuildArtifact {
    BuildArtifact {
        id: artifact.id,
        run_id: run_id.to_string(),
        filename: artifact.filename,
        size_bytes: artifact.size,
        download_url: artifact.download_url,
        content_type: None,
        created_at: Utc::now(),
    }
}

/// Fetches available pipelines from all accessible organizations
pub(crate) async fn fetch_all_available_pipelines(
    client: &BuildkiteClient,
) -> PluginResult<Vec<AvailablePipeline>> {
    let organizations = client.fetch_organizations().await?;

    let futures = organizations
        .into_iter()
        .map(|org| client.fetch_org_pipelines(org.slug));

    let results = join_all(futures).await;

    let mut all_available_pipelines = Vec::new();
    for result in results {
        match result {
            Ok(mut pipelines) => all_available_pipelines.append(&mut pipelines),
            Err(_) => continue,
        }
    }

    Ok(all_available_pipelines)
}
