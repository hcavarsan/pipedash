use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;
use pipedash_plugin_api::*;

use crate::{
    client,
    config,
    mapper,
    metadata,
    types,
};

pub struct BitbucketPlugin {
    metadata: PluginMetadata,
    client: Option<client::BitbucketClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
}

impl Default for BitbucketPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl BitbucketPlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            client: None,
            provider_id: None,
            config: HashMap::new(),
        }
    }

    fn client(&self) -> PluginResult<&client::BitbucketClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }

    async fn fetch_all_repositories(&self) -> PluginResult<Vec<types::Repository>> {
        let client = self.client()?;
        let mut all_repos = Vec::new();
        let mut page = 1;

        const MAX_PAGES: usize = 50;

        loop {
            let params = PaginationParams {
                page,
                page_size: 100,
            };
            let response = client.list_all_repositories(&params).await?;

            if response.items.is_empty() {
                break;
            }

            all_repos.extend(response.items);

            if !response.has_more || page >= MAX_PAGES {
                break;
            }
            page += 1;
        }

        if let Some(selected) = config::parse_selected_items(&self.config) {
            Ok(all_repos
                .into_iter()
                .filter(|r| selected.contains(&r.full_name))
                .collect())
        } else {
            Ok(all_repos)
        }
    }

    async fn find_pipeline_by_build_number(
        &self, workspace: &str, repo_slug: &str, build_number: i64,
    ) -> PluginResult<types::Pipeline> {
        let client = self.client()?;
        let pipelines = client.list_pipelines(workspace, repo_slug, 100).await?;

        pipelines
            .into_iter()
            .find(|p| p.build_number == build_number)
            .ok_or_else(|| {
                PluginError::PipelineNotFound(format!(
                    "Pipeline run #{} not found in recent 100 runs for {}/{}",
                    build_number, workspace, repo_slug
                ))
            })
    }
}

#[async_trait]
impl Plugin for BitbucketPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn provider_type(&self) -> &str {
        "bitbucket"
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
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

        let client = http_client.unwrap_or_else(|| {
            std::sync::Arc::new(
                reqwest::Client::builder()
                    .use_rustls_tls()
                    .pool_max_idle_per_host(10)
                    .timeout(Duration::from_secs(30))
                    .connect_timeout(Duration::from_secs(10))
                    .tcp_keepalive(Duration::from_secs(60))
                    .build()
                    .expect("Failed to build HTTP client"),
            )
        });

        self.client = Some(client::BitbucketClient::new(client, api_url, auth_value));
        self.provider_id = Some(provider_id);
        self.config = config;

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client()?;
        client.get_user().await?;
        Ok(true)
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        let client = self.client()?;
        let workspaces = client.list_workspaces().await?;

        Ok(workspaces
            .into_iter()
            .map(|w| Organization {
                id: w.uuid,
                name: w.name,
                description: Some(w.slug),
            })
            .collect())
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        let params = params.unwrap_or_default();

        let response = client.list_all_repositories(&params).await?;

        let available = response
            .items
            .iter()
            .map(mapper::map_available_pipeline)
            .collect();

        Ok(PaginatedResponse::new(
            available,
            response.page,
            response.page_size,
            response.total_count,
        ))
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        let params = params.unwrap_or_default();

        let response = if let Some(workspace) = &org {
            client.list_repositories(workspace, &params).await?
        } else {
            client.list_all_repositories(&params).await?
        };

        let mut available: Vec<AvailablePipeline> = response
            .items
            .iter()
            .map(mapper::map_available_pipeline)
            .collect();

        if let Some(search_term) = search {
            let search_lower = search_term.to_lowercase();
            available.retain(|p| {
                p.name.to_lowercase().contains(&search_lower)
                    || p.id.to_lowercase().contains(&search_lower)
            });
        }

        Ok(PaginatedResponse::new(
            available,
            params.page,
            params.page_size,
            response.total_count,
        ))
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self
            .provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))?;

        let client = self.client()?;
        let repos = self.fetch_all_repositories().await?;

        let pipeline_futures = repos.iter().map(|repo| async move {
            let pipelines = client
                .list_pipelines(&repo.workspace.slug, &repo.slug, 1)
                .await
                .ok()?;
            let latest = pipelines.first();
            Some(mapper::map_pipeline(repo, latest, provider_id))
        });

        let results: Vec<Option<Pipeline>> = join_all(pipeline_futures).await;
        Ok(results.into_iter().flatten().collect())
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let (provider_id, workspace, repo_slug) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;

        let pipelines = client.list_pipelines(&workspace, &repo_slug, limit).await?;

        Ok(pipelines
            .iter()
            .map(|p| mapper::map_pipeline_run(p, &workspace, &repo_slug, provider_id))
            .collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let (provider_id, workspace, repo_slug) = config::parse_pipeline_id(pipeline_id)?;

        let pipeline = self
            .find_pipeline_by_build_number(&workspace, &repo_slug, run_number)
            .await?;

        Ok(mapper::map_pipeline_run(
            &pipeline,
            &workspace,
            &repo_slug,
            provider_id,
        ))
    }

    async fn fetch_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        Ok(vec![
            WorkflowParameter {
                name: "ref".to_string(),
                label: Some("Branch".to_string()),
                description: Some("Branch name to run pipeline on".to_string()),
                param_type: WorkflowParameterType::String {
                    default: Some("main".to_string()),
                },
                required: true,
            },
            WorkflowParameter {
                name: "custom_pipeline".to_string(),
                label: Some("Custom Pipeline".to_string()),
                description: Some("Name of custom pipeline to run (optional)".to_string()),
                param_type: WorkflowParameterType::String { default: None },
                required: false,
            },
        ])
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let (_, workspace, repo_slug) = config::parse_pipeline_id(&params.workflow_id)?;
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

        let pipeline = client
            .trigger_pipeline(&workspace, &repo_slug, request)
            .await?;

        let url = pipeline.links.html.map(|l| l.href).unwrap_or_else(|| {
            format!(
                "https://bitbucket.org/{}/{}/pipelines/results/{}",
                workspace, repo_slug, pipeline.build_number
            )
        });

        Ok(url)
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let (_, workspace, repo_slug) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;

        let pipeline = self
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

    async fn fetch_agents(&self) -> PluginResult<Vec<BuildAgent>> {
        Err(PluginError::NotSupported(
            "Bitbucket Pipelines uses Atlassian infrastructure - no agent monitoring".to_string(),
        ))
    }

    async fn fetch_artifacts(&self, _run_id: &str) -> PluginResult<Vec<BuildArtifact>> {
        Err(PluginError::NotSupported(
            "Artifact fetching not yet implemented for Bitbucket".to_string(),
        ))
    }

    async fn fetch_queues(&self) -> PluginResult<Vec<BuildQueue>> {
        Err(PluginError::NotSupported(
            "Build queues not supported by Bitbucket Pipelines".to_string(),
        ))
    }

    fn get_migrations(&self) -> Vec<String> {
        vec![]
    }
}
