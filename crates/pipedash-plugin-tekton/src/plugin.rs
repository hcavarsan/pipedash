use std::collections::HashMap;
use std::sync::OnceLock;

use async_trait::async_trait;
use futures::future::join_all;
use pipedash_plugin_api::*;

use crate::{client, config, mapper, types};

pub struct TektonPlugin {
    metadata: PluginMetadata,
    client: OnceLock<client::TektonClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
}

impl Default for TektonPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl TektonPlugin {
    pub fn new() -> Self {
        let default_kubeconfig = config::get_default_kubeconfig_path();

        let config_schema = ConfigSchema::new()
            .add_field(ConfigField {
                key: "kubeconfig_path".to_string(),
                label: "Kubeconfig Path".to_string(),
                description: Some(
                    "Path to your Kubernetes config file(s). Multiple paths can be separated by ':' (Unix) or ';' (Windows). Uses $KUBECONFIG env var if set."
                        .to_string(),
                ),
                field_type: ConfigFieldType::Text,
                required: false,
                default_value: Some(serde_json::Value::String(default_kubeconfig)),
                options: None,
                validation_regex: None,
                validation_message: None,
            })
            .add_field(ConfigField {
                key: "context".to_string(),
                label: "Kubernetes Context".to_string(),
                description: Some(
                    "Select a context from your kubeconfig. Leave empty to use current-context."
                        .to_string(),
                ),
                field_type: ConfigFieldType::Select,
                required: false,
                default_value: None,
                options: Some(Vec::new()),
                validation_regex: None,
                validation_message: None,
            });

        let metadata = PluginMetadata {
            name: "Tekton CD".to_string(),
            provider_type: "tekton".to_string(),
            version: "0.1.0".to_string(),
            description: "Monitor and trigger Tekton CI/CD pipelines running on Kubernetes"
                .to_string(),
            author: Some("Pipedash Team".to_string()),
            icon: Some("https://cdn.simpleicons.org/tekton/FD495C".to_string()),
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
            client: OnceLock::new(),
            provider_id: None,
            config: HashMap::new(),
        }
    }

    async fn client(&self) -> PluginResult<&client::TektonClient> {
        if let Some(client) = self.client.get() {
            return Ok(client);
        }

        let kubeconfig_path = config::get_kubeconfig_path(&self.config);
        let context = config::get_context(&self.config);

        let new_client = client::TektonClient::from_kubeconfig(
            kubeconfig_path.as_deref(),
            context.as_deref(),
        )
        .await?;

        Ok(self.client.get_or_init(|| new_client))
    }

    async fn fetch_all_pipelines_in_namespaces(
        &self,
    ) -> PluginResult<Vec<types::TektonPipeline>> {
        let client = self.client().await?;

        let selected_ids = config::get_selected_pipelines(&self.config);

        let namespaces = if selected_ids.is_empty() {
            client.list_namespaces_with_tekton().await?
        } else {
            let unique_namespaces: std::collections::HashSet<String> = selected_ids
                .iter()
                .filter_map(|id| {
                    let parts: Vec<&str> = id.split("__").collect();
                    if parts.len() >= 2 {
                        Some(parts[0].to_string())
                    } else {
                        None
                    }
                })
                .collect();
            unique_namespaces.into_iter().collect()
        };

        let pipeline_futures = namespaces.iter().map(|namespace| async move {
            client.list_pipelines(namespace).await.ok()
        });

        let results: Vec<Option<Vec<types::TektonPipeline>>> = join_all(pipeline_futures).await;

        let all_pipelines: Vec<types::TektonPipeline> = results
            .into_iter()
            .flatten()
            .flatten()
            .collect();

        if selected_ids.is_empty() {
            Ok(all_pipelines)
        } else {
            Ok(all_pipelines
                .into_iter()
                .filter(|p| {
                    let id = format!("{}__{}", p.metadata.namespace, p.metadata.name);
                    selected_ids.contains(&id)
                })
                .collect())
        }
    }

    async fn fetch_latest_run_for_pipeline(
        &self,
        namespace: &str,
        pipeline_name: &str,
    ) -> Option<types::TektonPipelineRun> {
        let client = self.client().await.ok()?;
        let mut runs = client
            .list_pipelineruns(namespace, Some(pipeline_name))
            .await
            .ok()?;

        runs.sort_by(|a, b| {
            let a_time = types::parse_timestamp(&a.metadata.creation_timestamp);
            let b_time = types::parse_timestamp(&b.metadata.creation_timestamp);
            b_time.cmp(&a_time)
        });

        runs.into_iter().next()
    }

    fn get_available_contexts(&self, kubeconfig_path: Option<&str>) -> PluginResult<Vec<String>> {
        use std::collections::HashSet;
        use std::path::PathBuf;

        let paths = if let Some(path_str) = kubeconfig_path {
            config::split_kubeconfig_paths(path_str)
        } else {
            let default_path = config::get_default_kubeconfig_path();
            config::split_kubeconfig_paths(&default_path)
        };

        let mut all_contexts = HashSet::new();

        for path_str in paths {
            let path = PathBuf::from(&path_str);
            if !path.exists() {
                continue;
            }

            match kube::config::Kubeconfig::read_from(&path) {
                Ok(kubeconfig) => {
                    for context in kubeconfig.contexts {
                        all_contexts.insert(context.name);
                    }
                }
                Err(_) => continue,
            }
        }

        if all_contexts.is_empty() {
            return Err(PluginError::InvalidConfig(
                "No valid kubeconfig files found or no contexts available".to_string(),
            ));
        }

        let mut contexts: Vec<String> = all_contexts.into_iter().collect();
        contexts.sort();
        Ok(contexts)
    }
}

#[async_trait]
impl Plugin for TektonPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn provider_type(&self) -> &str {
        "tekton"
    }

    fn initialize(
        &mut self,
        provider_id: i64,
        config: HashMap<String, String>,
    ) -> PluginResult<()> {
        self.provider_id = Some(provider_id);
        self.config = config;
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client().await?;

        let namespaces = client.list_namespaces_with_tekton().await?;

        if namespaces.is_empty() {
            return Err(PluginError::InvalidConfig(
                "No Tekton pipelines found in any accessible namespace".to_string(),
            ));
        }

        Ok(true)
    }

    async fn fetch_available_pipelines(&self) -> PluginResult<Vec<AvailablePipeline>> {
        let pipelines = self.fetch_all_pipelines_in_namespaces().await?;
        Ok(pipelines.iter().map(mapper::map_available_pipeline).collect())
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self
            .provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))?;

        let pipelines = self.fetch_all_pipelines_in_namespaces().await?;

        let pipeline_futures = pipelines.iter().map(|pipeline| async move {
            let latest_run = self
                .fetch_latest_run_for_pipeline(
                    &pipeline.metadata.namespace,
                    &pipeline.metadata.name,
                )
                .await;
            mapper::map_pipeline(pipeline, latest_run.as_ref(), provider_id)
        });

        let results = join_all(pipeline_futures).await;
        Ok(results)
    }

    async fn fetch_run_history(
        &self,
        pipeline_id: &str,
        limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let (provider_id, namespace, pipeline_name) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client().await?;

        let mut runs = client
            .list_pipelineruns(&namespace, Some(&pipeline_name))
            .await?;

        runs.sort_by(|a, b| {
            let a_time = types::parse_timestamp(&a.metadata.creation_timestamp);
            let b_time = types::parse_timestamp(&b.metadata.creation_timestamp);
            b_time.cmp(&a_time)
        });

        let limited_runs: Vec<types::TektonPipelineRun> =
            runs.into_iter().take(limit).collect();

        Ok(limited_runs
            .iter()
            .map(|run| mapper::map_pipeline_run(run, provider_id))
            .collect())
    }

    async fn fetch_run_details(
        &self,
        pipeline_id: &str,
        run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let (provider_id, namespace, _pipeline_name) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client().await?;

        let runs = client.list_pipelineruns(&namespace, None).await?;

        let run = runs
            .into_iter()
            .find(|r| {
                types::parse_timestamp(&r.metadata.creation_timestamp)
                    .map(|dt| dt.timestamp())
                    == Some(run_number)
            })
            .ok_or_else(|| {
                PluginError::PipelineNotFound(format!("PipelineRun with timestamp {} not found", run_number))
            })?;

        Ok(mapper::map_pipeline_run(&run, provider_id))
    }

    async fn fetch_workflow_parameters(
        &self,
        workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        let (_provider_id, namespace, pipeline_name) = config::parse_pipeline_id(workflow_id)?;
        let client = self.client().await?;

        let pipeline = client.get_pipeline(&namespace, &pipeline_name).await?;

        Ok(mapper::map_workflow_parameters(&pipeline))
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let (_provider_id, namespace, pipeline_name) =
            config::parse_pipeline_id(&params.workflow_id)?;

        let client = self.client().await?;

        let pipeline = client.get_pipeline(&namespace, &pipeline_name).await?;

        let param_values: Vec<types::ParamValue> = if let Some(inputs) = &params.inputs {
            inputs
                .as_object()
                .map(|obj| {
                    obj.iter()
                        .map(|(key, value)| types::ParamValue {
                            name: key.clone(),
                            value: value.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            vec![]
        };

        let workspaces: Vec<types::WorkspaceBinding> = pipeline
            .spec
            .workspaces
            .iter()
            .filter_map(|ws| {
                if ws.optional.unwrap_or(false) {
                    None
                } else {
                    Some(types::WorkspaceBinding {
                        name: ws.name.clone(),
                        empty_dir: Some(serde_json::json!({})),
                        persistent_volume_claim: None,
                        config_map: None,
                        secret: None,
                    })
                }
            })
            .collect();

        let run_name = format!("{}-{}", pipeline_name, chrono::Utc::now().timestamp());

        let pipelinerun = types::TektonPipelineRun {
            api_version: "tekton.dev/v1".to_string(),
            kind: "PipelineRun".to_string(),
            metadata: types::ObjectMeta {
                name: run_name.clone(),
                namespace: namespace.clone(),
                creation_timestamp: None,
                labels: HashMap::new(),
                annotations: HashMap::new(),
            },
            spec: types::PipelineRunSpec {
                pipeline_ref: Some(types::PipelineRef {
                    name: pipeline_name.clone(),
                }),
                params: param_values,
                workspaces,
                timeout: None,
            },
            status: types::PipelineRunStatus {
                conditions: vec![],
                start_time: None,
                completion_time: None,
                task_runs: HashMap::new(),
            },
        };

        let created_run = client.create_pipelinerun(&namespace, &pipelinerun).await?;

        Ok(format!(
            "PipelineRun created: {}/{}",
            namespace, created_run.metadata.name
        ))
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let (_provider_id, namespace, _pipeline_name) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client().await?;

        let runs = client.list_pipelineruns(&namespace, None).await?;

        let run = runs
            .into_iter()
            .find(|r| {
                r.metadata
                    .name
                    .split('-')
                    .last()
                    .and_then(|s| s.parse::<i64>().ok())
                    == Some(run_number)
            })
            .ok_or_else(|| {
                PluginError::PipelineNotFound(format!("PipelineRun with number {} not found", run_number))
            })?;

        client
            .delete_pipelinerun(&namespace, &run.metadata.name)
            .await?;

        Ok(())
    }

    async fn fetch_agents(&self) -> PluginResult<Vec<BuildAgent>> {
        Err(PluginError::NotSupported(
            "Build agents not supported by Tekton plugin".to_string(),
        ))
    }

    async fn fetch_artifacts(&self, _run_id: &str) -> PluginResult<Vec<BuildArtifact>> {
        Err(PluginError::NotSupported(
            "Artifacts not implemented for Tekton plugin".to_string(),
        ))
    }

    async fn fetch_queues(&self) -> PluginResult<Vec<BuildQueue>> {
        Err(PluginError::NotSupported(
            "Build queues not supported by Tekton plugin".to_string(),
        ))
    }

    fn get_migrations(&self) -> Vec<String> {
        vec![]
    }

    async fn get_field_options(
        &self,
        field_key: &str,
        config: &HashMap<String, String>,
    ) -> PluginResult<Vec<String>> {
        if field_key == "context" {
            let kubeconfig_path = config::get_kubeconfig_path(config);
            let contexts = self.get_available_contexts(kubeconfig_path.as_deref())?;
            Ok(contexts)
        } else {
            Ok(Vec::new())
        }
    }
}
