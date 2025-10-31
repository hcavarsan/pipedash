//! GitHub Actions plugin implementation

use std::collections::HashMap;

use async_trait::async_trait;
use futures::future::join_all;
use octocrab::Octocrab;
use pipedash_plugin_api::*;

use crate::{
    client,
    config,
};

/// GitHub Actions plugin for monitoring workflows and runs
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

impl GitHubPlugin {
    pub fn new() -> Self {
        // No config fields needed - repositories are selected in the UI
        let config_schema = ConfigSchema::new();

        let metadata = PluginMetadata {
            name: "GitHub Actions".to_string(),
            provider_type: "github".to_string(),
            version: "0.1.0".to_string(),
            description: "Monitor and trigger GitHub Actions workflows".to_string(),
            author: Some("Pipedash Team".to_string()),
            icon: Some("https://cdn.simpleicons.org/github/white".to_string()),
            config_schema,
            capabilities: PluginCapabilities {
                pipelines: true,
                pipeline_runs: true,
                trigger: true,
                agents: false,
                artifacts: false,
                queues: false,
                custom_tables: false,
            },
        };

        Self {
            metadata,
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
    ) -> PluginResult<()> {
        let token = config
            .get("token")
            .ok_or_else(|| PluginError::InvalidConfig("Missing GitHub token".to_string()))?;

        if token.is_empty() {
            eprintln!(
                "[GITHUB] ERROR: Token is empty for provider {}",
                provider_id
            );
            return Err(PluginError::InvalidConfig(
                "GitHub token is empty. Please check keyring permissions.".to_string(),
            ));
        }

        eprintln!("[GITHUB] Initializing with token length: {}", token.len());

        let octocrab = Octocrab::builder()
            .personal_token(token.clone())
            .build()
            .map_err(|e| {
                PluginError::InvalidConfig(format!("Failed to build GitHub client: {e}"))
            })?;

        self.client = Some(client::GitHubClient::new(octocrab));
        self.provider_id = Some(provider_id);
        self.config = config;

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client()?;

        // Try to fetch current user to validate credentials
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
    }

    async fn fetch_available_pipelines(&self) -> PluginResult<Vec<AvailablePipeline>> {
        let client = self.client()?;
        client.fetch_all_repositories().await
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self
            .provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))?;

        let repositories = config::get_repositories(&self.config);
        eprintln!("[GITHUB] Configured repositories: {repositories:?}");

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
                    eprintln!("[GITHUB] Repo returned {} workflows", pipelines.len());
                    all_pipelines.append(&mut pipelines);
                }
                Err(e) => errors.push(e),
            }
        }

        eprintln!(
            "[GITHUB] Total unique pipeline IDs: {}",
            all_pipelines
                .iter()
                .map(|p| &p.id)
                .collect::<std::collections::HashSet<_>>()
                .len()
        );
        eprintln!("[GITHUB] Total pipelines: {}", all_pipelines.len());

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

        let token = self
            .config
            .get("token")
            .ok_or_else(|| PluginError::Internal("Token not found".to_string()))?;

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

        let url = format!(
            "https://api.github.com/repos/{owner}/{repo}/actions/workflows/{workflow_id}/dispatches"
        );

        let http_client = reqwest::Client::new();
        let response = http_client
            .post(&url)
            .header("Authorization", format!("Bearer {token}"))
            .header("User-Agent", "pipedash")
            .header("Accept", "application/vnd.github.v3+json")
            .json(&body)
            .send()
            .await
            .map_err(|e| PluginError::ApiError(format!("Failed to trigger workflow: {e}")))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(PluginError::ApiError(format!(
                "Failed to trigger workflow: {error_text}"
            )));
        }

        let client = self.client()?;
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
                    eprintln!(
                        "[GITHUB] Found new run #{} after {} attempts",
                        run.run_number, attempt
                    );
                    new_run = Some(run);
                    break;
                }
            }

            if new_run.is_some() {
                break;
            }

            eprintln!("[GITHUB] Attempt {attempt}/10: Waiting for new run to appear...");
        }

        let (logs_url, run_number) = if let Some(run) = new_run {
            (run.html_url.to_string(), run.run_number)
        } else {
            // Fallback: return the latest run we found
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

        // First, fetch the run to get its ID
        let client = self.client()?;
        let run = client
            .fetch_run_by_number(owner, repo, workflow_id, run_number)
            .await?;

        // Now cancel using the run ID (convert RunId to u64)
        let run_id_u64: u64 = run.id.0;
        client.cancel_run(owner, repo, run_id_u64).await
    }
}
