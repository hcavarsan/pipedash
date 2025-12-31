use std::collections::HashMap;

use async_trait::async_trait;
use futures::future::join_all;
use octocrab::Octocrab;
use pipedash_plugin_api::*;

use crate::{
    client,
    config,
    metadata,
};

pub struct GitHubPlugin {
    metadata: PluginMetadata,
    client: Option<client::GitHubClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
}

impl Default for GitHubPlugin {
    fn default() -> Self {
        Self::new()
    }
}

mod permission_mapping {

    pub const FINE_GRAINED_FEATURE_MAPPINGS: &[(&str, &str)] = &[
        ("list_monitor_workflows", "Repository Metadata"),
        ("view_run_history", "Repository Metadata"),
        ("monitor_status", "Actions (Read)"),
        ("trigger_dispatch", "Actions (Write)"),
        ("cancel_workflows", "Actions (Write)"),
        ("access_org_repos", "Organization members and teams (Read)"),
    ];

    pub fn map_feature_permissions(
        token_type: &str, feature_id: &str, classic_permissions: &[String],
    ) -> Vec<String> {
        match token_type {
            "fine_grained" => FINE_GRAINED_FEATURE_MAPPINGS
                .iter()
                .find(|(id, _)| *id == feature_id)
                .map(|(_, perm)| vec![perm.to_string()])
                .unwrap_or_default(),
            _ => classic_permissions.to_vec(),
        }
    }
}

impl GitHubPlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            client: None,
            provider_id: None,
            config: HashMap::new(),
        }
    }

    fn client(&self) -> PluginResult<&client::GitHubClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }
}

#[async_trait]
impl Plugin for GitHubPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        _http_client: Option<std::sync::Arc<reqwest::Client>>,
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
        let api_url = config::build_api_url(&base_url);

        tracing::debug!(api_url = %api_url, "Using GitHub API URL");

        let octocrab = Octocrab::builder()
            .personal_token(token.clone())
            .base_uri(&api_url)
            .map_err(|e| PluginError::InvalidConfig(format!("Failed to set base URI: {e}")))?
            .build()
            .map_err(|e| {
                PluginError::InvalidConfig(format!("Failed to build GitHub client: {e}"))
            })?;

        let github_client = client::GitHubClient::new(octocrab, token.clone())?;
        self.client = Some(github_client);
        self.provider_id = Some(provider_id);
        self.config = config;

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client()?;

        client
            .retry_policy
            .retry(|| async {
                let result = client.octocrab.current().user().await;

                match result {
                    Ok(_) => Ok(true),
                    Err(e) => {
                        if e.to_string().contains("401") {
                            Err(PluginError::AuthenticationFailed(
                                "Invalid GitHub token".to_string(),
                            ))
                        } else {
                            Err(PluginError::ApiError(format!(
                                "Failed to validate credentials: {e}"
                            )))
                        }
                    }
                }
            })
            .await
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

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self
            .provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))?;

        let repositories = config::get_repositories(&self.config);
        tracing::debug!(repositories = ?repositories, "Configured GitHub repositories");

        if repositories.is_empty() {
            return Err(PluginError::InvalidConfig(
                "No repositories configured".to_string(),
            ));
        }

        let client = self.client()?;
        let futures = repositories
            .into_iter()
            .map(|repo_full_name| client.fetch_repo_workflows(provider_id, repo_full_name));

        let results = join_all(futures).await;

        let mut all_pipelines = Vec::new();
        let mut errors = Vec::new();

        for result in results {
            match result {
                Ok(mut pipelines) => {
                    tracing::debug!(count = pipelines.len(), "GitHub repo returned workflows");
                    all_pipelines.append(&mut pipelines);
                }
                Err(e) => errors.push(e),
            }
        }

        let unique_count = all_pipelines
            .iter()
            .map(|p| &p.id)
            .collect::<std::collections::HashSet<_>>()
            .len();
        tracing::debug!(
            unique_pipelines = unique_count,
            total_pipelines = all_pipelines.len(),
            "GitHub pipeline fetch complete"
        );

        if !errors.is_empty() && all_pipelines.is_empty() {
            return Err(errors.into_iter().next().unwrap());
        }

        Ok(all_pipelines)
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 5 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {} (expected 5 parts, got {})",
                pipeline_id,
                parts.len()
            )));
        }

        let owner = parts[2];
        let repo = parts[3];
        let workflow_id_str = parts[4];
        let workflow_id: u64 = workflow_id_str.parse().map_err(|_| {
            PluginError::InvalidConfig(format!("Invalid workflow ID: {workflow_id_str}"))
        })?;

        let client = self.client()?;
        let runs = client
            .fetch_run_history(owner, repo, workflow_id, limit)
            .await?;

        let pipeline_runs = runs
            .into_iter()
            .map(|run| client::run_to_pipeline_run(run, pipeline_id))
            .collect();

        Ok(pipeline_runs)
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 5 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {} (expected 5 parts, got {})",
                pipeline_id,
                parts.len()
            )));
        }

        let owner = parts[2];
        let repo = parts[3];
        let workflow_id_str = parts[4];
        let workflow_id: u64 = workflow_id_str.parse().map_err(|_| {
            PluginError::InvalidConfig(format!("Invalid workflow ID: {workflow_id_str}"))
        })?;

        let client = self.client()?;
        let run = client
            .fetch_run_by_number(owner, repo, workflow_id, run_number)
            .await?;

        Ok(client::run_to_pipeline_run(run, pipeline_id))
    }

    async fn fetch_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        Ok(vec![WorkflowParameter {
            name: "ref".to_string(),
            label: Some("Ref".to_string()),
            description: Some("Branch, tag, or commit SHA to run workflow on".to_string()),
            param_type: WorkflowParameterType::String {
                default: Some("main".to_string()),
            },
            required: true,
        }])
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let parts: Vec<&str> = params.workflow_id.split("__").collect();
        if parts.len() != 5 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid workflow ID format: {} (expected 5 parts, got {})",
                params.workflow_id,
                parts.len()
            )));
        }

        let owner = parts[2];
        let repo = parts[3];
        let workflow_id = parts[4];

        let ref_value = params
            .inputs
            .as_ref()
            .and_then(|inputs| inputs.get("ref"))
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string();

        let mut body = serde_json::json!({
            "ref": &ref_value,
        });

        if let Some(inputs) = params.inputs {
            if let Some(obj) = inputs.as_object() {
                let workflow_inputs: serde_json::Map<String, serde_json::Value> = obj
                    .iter()
                    .filter(|(k, _)| k.as_str() != "ref")
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();

                if !workflow_inputs.is_empty() {
                    body["inputs"] = serde_json::Value::Object(workflow_inputs);
                }
            }
        }

        let client = self.client()?;
        let url = format!("/repos/{owner}/{repo}/actions/workflows/{workflow_id}/dispatches");

        let response: Result<serde_json::Value, octocrab::Error> =
            client.octocrab.post(url, Some(&body)).await;

        if let Err(e) = response {
            return Err(PluginError::ApiError(format!(
                "Failed to trigger workflow: {e}"
            )));
        }

        let workflow_id_u64: u64 = workflow_id.parse().map_err(|_| {
            PluginError::InvalidConfig(format!("Invalid workflow ID: {workflow_id}"))
        })?;

        let trigger_time = chrono::Utc::now();

        let previous_runs = client
            .fetch_run_history(owner, repo, workflow_id_u64, 5)
            .await?;
        let previous_latest_run_number = previous_runs.first().map(|r| r.run_number).unwrap_or(0);

        let mut new_run = None;
        for attempt in 1..=10 {
            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

            let runs = client
                .fetch_run_history(owner, repo, workflow_id_u64, 5)
                .await?;

            for run in runs {
                if run.run_number > previous_latest_run_number
                    && run.head_branch == ref_value
                    && run.created_at.with_timezone(&chrono::Utc) >= trigger_time
                {
                    tracing::debug!(
                        run_number = run.run_number,
                        attempt = attempt,
                        "Found new GitHub run"
                    );
                    new_run = Some(run);
                    break;
                }
            }

            if new_run.is_some() {
                break;
            }

            tracing::debug!(
                attempt = attempt,
                max_attempts = 10,
                "Waiting for new GitHub run to appear"
            );
        }

        let (logs_url, run_number) = if let Some(run) = new_run {
            (run.html_url.to_string(), run.run_number)
        } else {
            let runs = client
                .fetch_run_history(owner, repo, workflow_id_u64, 1)
                .await?;
            (
                runs.first()
                    .map(|r| r.html_url.to_string())
                    .unwrap_or_default(),
                runs.first().map(|r| r.run_number).unwrap_or(0),
            )
        };

        Ok(serde_json::json!({
            "message": format!("Triggered workflow on ref {}", ref_value),
            "run_number": run_number,
            "logs_url": logs_url
        })
        .to_string())
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 5 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {} (expected 5 parts, got {})",
                pipeline_id,
                parts.len()
            )));
        }

        let owner = parts[2];
        let repo = parts[3];
        let workflow_id_str = parts[4];
        let workflow_id: u64 = workflow_id_str.parse().map_err(|_| {
            PluginError::InvalidConfig(format!("Invalid workflow ID: {workflow_id_str}"))
        })?;

        let client = self.client()?;
        let run = client
            .fetch_run_by_number(owner, repo, workflow_id, run_number)
            .await?;

        let run_id_u64: u64 = run.id.0;
        client.cancel_run(owner, repo, run_id_u64).await
    }
    async fn check_permissions(&self) -> PluginResult<PermissionStatus> {
        let client = self.client()?;
        client.check_token_permissions().await
    }

    fn get_feature_availability(&self, status: &PermissionStatus) -> Vec<FeatureAvailability> {
        use permission_mapping::map_feature_permissions;

        let features = &self.metadata().features;

        let token_type = status
            .metadata
            .get("token_type")
            .map(|s| s.as_str())
            .unwrap_or("classic_pat");

        let granted_perms: std::collections::HashSet<String> = status
            .permissions
            .iter()
            .filter(|p| p.granted)
            .map(|p| p.permission.name.clone())
            .collect();

        features
            .iter()
            .map(|feature| {
                let mapped_required =
                    map_feature_permissions(token_type, &feature.id, &feature.required_permissions);

                let missing: Vec<String> = mapped_required
                    .iter()
                    .filter(|perm| !granted_perms.contains(*perm))
                    .cloned()
                    .collect();

                let transformed_feature = Feature {
                    id: feature.id.clone(),
                    name: feature.name.clone(),
                    description: feature.description.clone(),
                    required_permissions: mapped_required,
                };

                FeatureAvailability {
                    feature: transformed_feature,
                    available: missing.is_empty(),
                    missing_permissions: missing,
                }
            })
            .collect()
    }
}
