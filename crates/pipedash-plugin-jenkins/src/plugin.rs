//! Jenkins plugin implementation

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
};

/// Jenkins plugin for monitoring jobs and builds
pub struct JenkinsPlugin {
    metadata: PluginMetadata,
    client: Option<client::JenkinsClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
}

impl Default for JenkinsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl JenkinsPlugin {
    pub fn new() -> Self {
        let mut config_schema = ConfigSchema::new();

        config_schema = config_schema.add_field(ConfigField {
            key: "server_url".to_string(),
            label: "Jenkins Server URL".to_string(),
            description: Some(
                "Your Jenkins server URL (e.g., https://jenkins.example.com)".to_string(),
            ),
            field_type: ConfigFieldType::Text,
            required: true,
            default_value: None,
            options: None,
            validation_regex: None,
            validation_message: None,
        });

        config_schema = config_schema.add_field(ConfigField {
            key: "username".to_string(),
            label: "Username".to_string(),
            description: Some("Your Jenkins username".to_string()),
            field_type: ConfigFieldType::Text,
            required: true,
            default_value: None,
            options: None,
            validation_regex: None,
            validation_message: None,
        });

        let metadata = PluginMetadata {
            name: "Jenkins".to_string(),
            provider_type: "jenkins".to_string(),
            version: "0.1.0".to_string(),
            description: "Monitor Jenkins jobs, builds, and pipelines".to_string(),
            author: Some("Pipedash Team".to_string()),
            icon: Some("https://www.jenkins.io/favicon.ico".to_string()),
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

    fn client(&self) -> PluginResult<&client::JenkinsClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }
}

#[async_trait]
impl Plugin for JenkinsPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
    ) -> PluginResult<()> {
        let token = config
            .get("token")
            .ok_or_else(|| PluginError::InvalidConfig("Missing Jenkins API token".to_string()))?;

        let username = config
            .get("username")
            .ok_or_else(|| PluginError::InvalidConfig("Missing Jenkins username".to_string()))?;

        let auth_value = format!("{username}:{token}");
        let auth_header = format!(
            "Basic {}",
            base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                auth_value.as_bytes()
            )
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&auth_header)
                .map_err(|e| PluginError::InvalidConfig(format!("Invalid auth format: {e}")))?,
        );

        let server_url = config
            .get("server_url")
            .ok_or_else(|| PluginError::InvalidConfig("Missing server_url".to_string()))?
            .trim_end_matches('/')
            .to_string();

        let http_client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| PluginError::Internal(format!("Failed to build HTTP client: {e}")))?;

        self.client = Some(client::JenkinsClient::new(http_client, server_url));
        self.provider_id = Some(provider_id);
        self.config = config;

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client()?;
        let url = format!("{}/api/json", client.server_url());

        let response = client
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| PluginError::NetworkError(format!("Failed to connect: {e}")))?;

        if response.status().is_success() {
            Ok(true)
        } else if response.status() == 401 || response.status() == 403 {
            Err(PluginError::AuthenticationFailed(
                "Invalid Jenkins credentials".to_string(),
            ))
        } else {
            Err(PluginError::ApiError(format!(
                "API error: {}",
                response.status()
            )))
        }
    }

    async fn fetch_available_pipelines(&self) -> PluginResult<Vec<AvailablePipeline>> {
        let client = self.client()?;
        let all_jobs = client.discover_all_jobs().await?;
        Ok(client.discovered_jobs_to_available_pipelines(all_jobs))
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self
            .provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))?;

        let job_paths = config::parse_selected_items(&self.config)?;

        if job_paths.is_empty() {
            return Err(PluginError::InvalidConfig("No jobs configured".to_string()));
        }

        let client = self.client()?;
        let futures = job_paths
            .iter()
            .map(|job_path| client.fetch_pipeline(provider_id, job_path.clone()));

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
        if parts.len() != 3 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {pipeline_id}"
            )));
        }

        let job_path = parts[2];
        let client = self.client()?;
        let builds = client.fetch_build_history(job_path, limit).await?;

        let pipeline_runs = builds
            .into_iter()
            .map(|build| {
                let encoded_path = config::encode_job_name(job_path);
                mapper::build_to_pipeline_run(
                    build,
                    pipeline_id,
                    client.server_url(),
                    &encoded_path,
                )
            })
            .collect();

        Ok(pipeline_runs)
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 3 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {pipeline_id}"
            )));
        }

        let job_path = parts[2];
        let client = self.client()?;
        let build = client.fetch_build_details(job_path, run_number).await?;

        let encoded_path = config::encode_job_name(job_path);
        Ok(mapper::build_to_pipeline_run(
            build,
            pipeline_id,
            client.server_url(),
            &encoded_path,
        ))
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let parts: Vec<&str> = params.workflow_id.split("__").collect();
        if parts.len() != 3 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid workflow ID format: {}",
                params.workflow_id
            )));
        }

        let job_path = parts[2];

        let mut form_data = Vec::new();
        if let Some(inputs) = params.inputs {
            if let Some(obj) = inputs.as_object() {
                for (k, v) in obj.iter() {
                    if v.is_null() {
                        continue;
                    }

                    if v.is_array() {
                        if let Some(arr) = v.as_array() {
                            for item in arr {
                                if item.is_null() {
                                    continue;
                                }
                                let value_str = if item.is_boolean() {
                                    item.as_bool().unwrap().to_string()
                                } else if item.is_number() {
                                    item.to_string()
                                } else {
                                    item.as_str()
                                        .map(|s| s.to_string())
                                        .unwrap_or_else(|| item.to_string())
                                };
                                form_data.push((k.clone(), value_str));
                            }
                        }
                    } else if v.is_boolean() {
                        form_data.push((k.clone(), v.as_bool().unwrap().to_string()));
                    } else if v.is_number() {
                        form_data.push((k.clone(), v.to_string()));
                    } else if let Some(s) = v.as_str() {
                        form_data.push((k.clone(), s.to_string()));
                    }
                }
            }
        }

        if form_data.is_empty() {
            form_data.push(("json".to_string(), serde_json::json!({}).to_string()));
        }

        let client = self.client()?;
        client.trigger_build(job_path, form_data).await?;

        Ok(serde_json::json!({
            "message": format!("Triggered build for job {job_path}"),
            "job_path": job_path
        })
        .to_string())
    }

    async fn fetch_workflow_parameters(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        let start = std::time::Instant::now();
        eprintln!("[JENKINS] Fetching parameters for workflow: {workflow_id}");

        let parts: Vec<&str> = workflow_id.split("__").collect();
        if parts.len() != 3 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid workflow ID format: {workflow_id}"
            )));
        }

        let job_path = parts[2];
        let client = self.client()?;
        let response = client.fetch_job_parameters(job_path).await?;

        let param_definitions: Vec<_> = response
            .property
            .into_iter()
            .filter(|prop| {
                prop._class
                    .as_ref()
                    .map(|c| {
                        c.contains("ParametersDefinitionProperty")
                            || c.contains("ParametersProperty")
                    })
                    .unwrap_or(true)
            })
            .flat_map(|prop| prop.parameter_definitions)
            .collect();

        let parameters = mapper::parameter_definitions_to_workflow_parameters(param_definitions);

        eprintln!(
            "[JENKINS] Processed {} parameters in {:?}",
            parameters.len(),
            start.elapsed()
        );
        Ok(parameters)
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let parts: Vec<&str> = pipeline_id.split("__").collect();
        if parts.len() != 3 {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid pipeline ID format: {pipeline_id}"
            )));
        }

        let job_path = parts[2];
        let client = self.client()?;
        client.cancel_build(job_path, run_number).await
    }
}
