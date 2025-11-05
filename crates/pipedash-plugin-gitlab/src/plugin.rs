use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;
use pipedash_plugin_api::*;
use reqwest::header::{
    HeaderMap,
    HeaderValue,
};

use crate::{
    client,
    config,
    mapper,
    metadata,
    types,
};

pub struct GitLabPlugin {
    metadata: PluginMetadata,
    client: Option<client::GitLabClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
}

impl Default for GitLabPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl GitLabPlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            client: None,
            provider_id: None,
            config: HashMap::new(),
        }
    }

    fn client(&self) -> PluginResult<&client::GitLabClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }

    async fn fetch_all_projects(&self) -> PluginResult<Vec<types::Project>> {
        let client = self.client()?;
        let mut all_projects = Vec::new();
        let mut page = 1;

        loop {
            let projects = client.list_projects(page).await?;
            if projects.is_empty() {
                break;
            }
            all_projects.extend(projects);
            page += 1;

            if page > 100 {
                break;
            }
        }

        if let Some(selected_paths) = config::parse_selected_items(&self.config) {
            Ok(all_projects
                .into_iter()
                .filter(|p| {
                    let normalized = p.name_with_namespace.replace(" ", "");
                    selected_paths.contains(&normalized)
                })
                .collect())
        } else {
            Ok(all_projects)
        }
    }
}

#[async_trait]
impl Plugin for GitLabPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn provider_type(&self) -> &str {
        "gitlab"
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
    ) -> PluginResult<()> {
        let token = config
            .get("token")
            .ok_or_else(|| PluginError::InvalidConfig("Missing GitLab access token".to_string()))?;

        let base_url = config::get_base_url(&config);
        let api_url = config::build_api_url(&base_url);

        let mut headers = HeaderMap::new();
        headers.insert(
            "PRIVATE-TOKEN",
            HeaderValue::from_str(token)
                .map_err(|e| PluginError::InvalidConfig(format!("Invalid token format: {}", e)))?,
        );

        let http_client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| PluginError::Internal(format!("Failed to build HTTP client: {}", e)))?;

        self.client = Some(client::GitLabClient::new(http_client, api_url));
        self.provider_id = Some(provider_id);
        self.config = config;

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client()?;
        client.get_user().await?;
        Ok(true)
    }

    async fn fetch_available_pipelines(&self) -> PluginResult<Vec<AvailablePipeline>> {
        let projects = self.fetch_all_projects().await?;
        Ok(projects
            .iter()
            .map(mapper::map_available_pipeline)
            .collect())
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self
            .provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))?;

        let client = self.client()?;
        let projects = self.fetch_all_projects().await?;

        let pipeline_futures = projects.iter().map(|project| async move {
            let pipelines = client.get_project_pipelines(project.id, 1).await.ok()?;
            let latest_pipeline = pipelines.first();
            Some(mapper::map_pipeline(project, latest_pipeline, provider_id))
        });

        let results: Vec<Option<Pipeline>> = join_all(pipeline_futures).await;
        Ok(results.into_iter().flatten().collect())
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let (provider_id, project_id) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;

        let project = client.get_project(project_id).await?;

        let pipeline_list = client.get_project_pipelines(project_id, limit).await?;

        let parts: Vec<&str> = project.name_with_namespace.split('/').collect();
        let namespace = if parts.len() >= 2 {
            Some(parts[..parts.len() - 1].join("/"))
        } else {
            None
        };

        let detailed_pipeline_futures = pipeline_list
            .iter()
            .map(|p| async move { client.get_pipeline(project_id, p.id).await });

        let detailed_pipelines = join_all(detailed_pipeline_futures).await;

        Ok(detailed_pipelines
            .into_iter()
            .filter_map(|result| result.ok())
            .map(|p| mapper::map_pipeline_run(&p, project_id, provider_id, namespace.as_deref()))
            .collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let (provider_id, project_id) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;

        let project = client.get_project(project_id).await?;
        let pipeline = client.get_pipeline(project_id, run_number).await?;

        let parts: Vec<&str> = project.name_with_namespace.split('/').collect();
        let namespace = if parts.len() >= 2 {
            Some(parts[..parts.len() - 1].join("/"))
        } else {
            None
        };

        Ok(mapper::map_pipeline_run(
            &pipeline,
            project_id,
            provider_id,
            namespace.as_deref(),
        ))
    }

    async fn fetch_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        Ok(vec![WorkflowParameter {
            name: "ref".to_string(),
            label: Some("Ref".to_string()),
            description: Some("Branch, tag, or commit SHA to run pipeline on".to_string()),
            param_type: WorkflowParameterType::String {
                default: Some("main".to_string()),
            },
            required: true,
        }])
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let (_provider_id, project_id) = config::parse_pipeline_id(&params.workflow_id)?;

        let client = self.client()?;

        let ref_name = params
            .inputs
            .as_ref()
            .and_then(|inputs| inputs.get("ref"))
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string();

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

        let pipeline = client
            .trigger_pipeline(project_id, ref_name, variables)
            .await?;
        Ok(pipeline.web_url)
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let (_, project_id) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        client.cancel_pipeline(project_id, run_number).await?;
        Ok(())
    }

    async fn fetch_agents(&self) -> PluginResult<Vec<BuildAgent>> {
        Err(PluginError::NotSupported(
            "GitLab runners monitoring not implemented".to_string(),
        ))
    }

    async fn fetch_artifacts(&self, _run_id: &str) -> PluginResult<Vec<BuildArtifact>> {
        Err(PluginError::NotSupported(
            "Artifacts download not implemented".to_string(),
        ))
    }

    async fn fetch_queues(&self) -> PluginResult<Vec<BuildQueue>> {
        Err(PluginError::NotSupported(
            "Build queues not supported by GitLab".to_string(),
        ))
    }

    fn get_migrations(&self) -> Vec<String> {
        vec![]
    }
}
