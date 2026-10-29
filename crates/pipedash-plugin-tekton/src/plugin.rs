use std::collections::HashMap;

use async_trait::async_trait;
use futures::future::{
    join_all,
    try_join,
    try_join_all,
};
use k8s_openapi::api::apps::v1::Deployment;
use pipedash_plugin_api::*;
use tokio::sync::OnceCell;

use crate::{
    client,
    config,
    log_dialect::{
        self,
        TektonLogDialect,
    },
    mapper,
    metadata,
    step_log::{
        self,
        ContainerCursor,
        StartedStep,
        TektonCursor,
    },
    types,
};

const CONTROLLER_DEPLOYMENTS: [&str; 2] =
    ["tekton-pipelines-controller", "tekton-pipelines-webhook"];

pub struct TektonPlugin {
    metadata: PluginMetadata,
    client: OnceCell<client::TektonClient>,
    status_source: Option<StatusSource>,
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
        Self {
            metadata: metadata::create_metadata(),
            client: OnceCell::new(),
            status_source: None,
            provider_id: None,
            config: HashMap::new(),
        }
    }

    async fn client(&self) -> PluginResult<&client::TektonClient> {
        self.client
            .get_or_try_init(|| async {
                let source = config::get_kubeconfig_source(&self.config);
                let context = config::get_context(&self.config);

                client::TektonClient::from_kubeconfig(&source, context.as_deref()).await
            })
            .await
    }

    async fn fetch_all_pipelines_in_namespaces(&self) -> PluginResult<Vec<types::TektonPipeline>> {
        let client = self.client().await?;

        let selected_ids = config::get_selected_pipelines(&self.config);

        let namespaces = if selected_ids.is_empty() {
            let namespace_mode = config::get_namespace_mode(&self.config);

            match namespace_mode {
                config::NamespaceMode::Custom => config::get_namespaces(&self.config),
                config::NamespaceMode::All => client.list_namespaces_with_pipelines().await?,
            }
        } else {
            let unique_namespaces: std::collections::HashSet<String> = selected_ids
                .iter()
                .filter_map(|id| {
                    id.split_once("__")
                        .map(|(namespace, _)| namespace.to_string())
                })
                .collect();
            unique_namespaces.into_iter().collect()
        };

        let all_pipelines: Vec<types::TektonPipeline> = try_join_all(
            namespaces
                .iter()
                .map(|namespace| client.list_pipelines(namespace)),
        )
        .await?
        .into_iter()
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
        &self, namespace: &str, pipeline_name: &str,
    ) -> PluginResult<Option<types::TektonPipelineRun>> {
        let client = self.client().await?;
        let mut runs = match client
            .list_pipelineruns(namespace, Some(pipeline_name))
            .await
        {
            Ok(runs) => runs,
            Err(PluginError::PipelineNotFound(message)) => {
                tracing::warn!(namespace, pipeline_name, %message, "PipelineRuns not found for pipeline");
                return Ok(None);
            }
            Err(error) => return Err(error),
        };

        runs.sort_by(|a, b| {
            let a_time = types::parse_timestamp(&a.metadata.creation_timestamp);
            let b_time = types::parse_timestamp(&b.metadata.creation_timestamp);
            b_time.cmp(&a_time)
        });

        Ok(runs.into_iter().next())
    }

    fn get_available_contexts(
        &self, source: &config::KubeconfigSource,
    ) -> PluginResult<Vec<String>> {
        use std::collections::HashSet;
        use std::path::PathBuf;

        let mut all_contexts = HashSet::new();

        match source {
            config::KubeconfigSource::Content(content) => {
                for context in client::parse_kubeconfig_content(content)?.contexts {
                    all_contexts.insert(context.name);
                }
            }
            config::KubeconfigSource::Path(kubeconfig_path) => {
                let paths = if let Some(path_str) = kubeconfig_path {
                    config::split_kubeconfig_paths(path_str)
                } else {
                    let default_path = config::get_default_kubeconfig_path();
                    config::split_kubeconfig_paths(&default_path)
                };

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

fn find_run(
    runs: Vec<types::TektonPipelineRun>, run_number: i64,
) -> PluginResult<types::TektonPipelineRun> {
    let mut matching: Vec<_> = runs
        .into_iter()
        .filter(|r| mapper::run_number(&r.metadata) == run_number)
        .collect();

    match matching.len() {
        0 => Err(PluginError::PipelineNotFound(format!(
            "PipelineRun #{run_number} not found"
        ))),
        1 => Ok(matching.remove(0)),
        _ => {
            let names: Vec<_> = matching.iter().map(|r| r.metadata.name.as_str()).collect();
            Err(PluginError::ApiError(format!(
                "PipelineRuns {} share run number {run_number}. Use kubectl to pick one",
                names.join(", ")
            )))
        }
    }
}

fn run_target<'a>(run: &RunRef<'a>) -> PluginResult<(&'a str, &'a str)> {
    let metadata = run.metadata;
    let field = |key: &str| {
        metadata
            .get(key)
            .and_then(|value| value.as_str())
            .filter(|value| config::is_kube_name(value))
            .ok_or_else(|| {
                PluginError::InvalidConfig(format!("Run {} has no valid {key}", run.run_id))
            })
    };
    Ok((field("namespace")?, field("pipelinerun_name")?))
}

async fn pipe_steps(
    client: &client::TektonClient, namespace: &str, pod: &str, uid: &str,
    steps: &[StartedStep<'_>], prior: Option<&TektonCursor>, sink: &mut LogSink,
) -> PluginResult<Option<TektonCursor>> {
    let mut containers = Vec::with_capacity(steps.len());
    for (index, started) in steps.iter().enumerate() {
        let before = prior.and_then(|prior| prior.containers.get(index));
        if let Some(before) = before.filter(|before| before.done) {
            containers.push(before.clone());
            continue;
        }
        if before.is_none() {
            sink.write(
                log_dialect::step_header(&started.step.name, started.step.started_at()).into(),
            )
            .await?;
        }
        let Some(read) = client
            .stream_step_log(
                namespace,
                pod,
                &started.container,
                before,
                !started.done,
                sink,
            )
            .await?
        else {
            return Ok(None);
        };
        let capped = before.is_none() && read.lines >= client::STEP_LOG_TAIL_LINES;
        if capped {
            sink.write(log_dialect::truncation_notice(client::STEP_LOG_TAIL_LINES).into())
                .await?;
        }
        containers.push(ContainerCursor {
            name: started.container.clone(),
            instance: started.instance.clone(),
            done: started.done,
            capped,
            last: read.last,
        });
    }
    Ok(Some(TektonCursor {
        pod: uid.to_string(),
        containers,
    }))
}

fn classify_controller(name: &str, deployment: &Deployment) -> Option<ComponentHealth> {
    let desired = deployment.spec.as_ref().and_then(|spec| spec.replicas);
    let status = deployment.status.as_ref();
    let available = status.and_then(|status| status.available_replicas);
    let unavailable = status
        .and_then(|status| status.conditions.as_ref())
        .is_some_and(|conditions| {
            conditions
                .iter()
                .any(|condition| condition.type_ == "Available" && condition.status == "False")
        });

    let (level, detail) = match (desired, available) {
        (Some(0), _) => (HealthLevel::MajorOutage, "scaled to zero".to_string()),
        (_, Some(0)) => (HealthLevel::MajorOutage, "not running".to_string()),
        (Some(desired), Some(available)) if available < desired => (
            HealthLevel::Degraded,
            format!("{available}/{desired} replicas available"),
        ),
        (_, None) if unavailable => (HealthLevel::MajorOutage, "not available".to_string()),
        _ => return None,
    };

    Some(ComponentHealth {
        name: name.to_string(),
        level,
        detail: Some(detail),
    })
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
        &mut self, provider_id: i64, config: HashMap<String, String>,
        _http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let status_source = custom_status_source(&config)?;
        self.provider_id = Some(provider_id);
        self.config = config;
        self.status_source = status_source;
        self.client = OnceCell::new();
        Ok(())
    }

    fn status_source(&self) -> Option<&StatusSource> {
        self.status_source.as_ref()
    }

    async fn check_health(&self) -> PluginResult<Vec<ComponentHealth>> {
        let client = self.client().await?;
        let namespace = config::get_controller_namespace(&self.config);

        let checks = join_all(CONTROLLER_DEPLOYMENTS.iter().map(|name| async {
            client
                .get_deployment(&namespace, name)
                .await
                .map(|deployment| classify_controller(name, &deployment))
        }))
        .await;

        collect_controller_health(checks)
    }

    async fn check_permissions(&self) -> PluginResult<PermissionStatus> {
        let client = self.client().await?;
        let namespace = config::get_controller_namespace(&self.config);

        let allowed = try_join_all(
            CONTROLLER_DEPLOYMENTS
                .iter()
                .map(|name| client.can_i(&namespace, "apps", "deployments", "get", name)),
        )
        .await?;

        let permissions = vec![PermissionCheck {
            permission: metadata::controller_permission(),
            granted: allowed.into_iter().all(|allowed| allowed),
        }];
        let all_granted = permissions
            .iter()
            .filter(|check| check.permission.required)
            .all(|check| check.granted);

        Ok(PermissionStatus {
            permissions,
            all_granted,
            checked_at: chrono::Utc::now(),
            metadata: HashMap::new(),
        })
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client().await?;
        let namespace_mode = config::get_namespace_mode(&self.config);

        let namespaces = match namespace_mode {
            config::NamespaceMode::Custom => {
                let manual_namespaces = config::get_namespaces(&self.config);

                if manual_namespaces.is_empty() {
                    return Err(PluginError::InvalidConfig(
                        "Namespace mode is set to 'custom' but no namespaces are specified. Please provide at least one namespace in the 'namespaces' field (e.g., 'default,tekton-pipelines').".to_string(),
                    ));
                }

                client
                    .validate_namespaces_have_pipelines(&manual_namespaces)
                    .await?
            }
            config::NamespaceMode::All => match client.try_list_namespaces_cluster_wide().await {
                Ok(all_namespaces) => {
                    if all_namespaces.is_empty() {
                        return Err(PluginError::InvalidConfig(
                                "No namespaces found in the cluster. Please verify your cluster connection and permissions.".to_string(),
                            ));
                    }
                    client.list_namespaces_with_pipelines().await?
                }
                Err(e) => return Err(e),
            },
        };

        if namespaces.is_empty() {
            let hint = match namespace_mode {
                config::NamespaceMode::Custom => "Verify that the specified namespaces exist and contain Tekton pipelines, and that you have permissions to access them.",
                config::NamespaceMode::All => "Try switching to 'custom' namespace mode and manually specify the namespaces containing your Tekton pipelines.",
            };

            return Err(PluginError::InvalidConfig(format!(
                "No Tekton pipelines found in any accessible namespace. {}",
                hint
            )));
        }

        Ok(true)
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let params = params.unwrap_or_default();
        let pipelines = self.fetch_all_pipelines_in_namespaces().await?;
        let all_pipelines: Vec<_> = pipelines
            .iter()
            .map(mapper::map_available_pipeline)
            .collect();

        let total_count = all_pipelines.len();
        let start = ((params.page - 1) * params.page_size).min(total_count);
        let end = (start + params.page_size).min(total_count);
        let items = all_pipelines[start..end].to_vec();

        Ok(PaginatedResponse::new(
            items,
            params.page,
            params.page_size,
            total_count,
        ))
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self
            .provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))?;

        let pipelines = self.fetch_all_pipelines_in_namespaces().await?;

        try_join_all(pipelines.iter().map(|pipeline| async move {
            let latest_run = self
                .fetch_latest_run_for_pipeline(
                    &pipeline.metadata.namespace,
                    &pipeline.metadata.name,
                )
                .await?;
            Ok(mapper::map_pipeline(
                pipeline,
                latest_run.as_ref(),
                provider_id,
            ))
        }))
        .await
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
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

        let limited_runs: Vec<types::TektonPipelineRun> = runs.into_iter().take(limit).collect();

        Ok(limited_runs
            .iter()
            .map(|run| mapper::map_pipeline_run(run, provider_id))
            .collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let (provider_id, namespace, pipeline_name) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client().await?;

        let runs = client
            .list_pipelineruns(&namespace, Some(&pipeline_name))
            .await?;

        let run = find_run(runs, run_number)?;

        Ok(mapper::map_pipeline_run(&run, provider_id))
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let (namespace, pipelinerun_name) = run_target(&run)?;
        let client = self.client().await?;

        let (pipelinerun, task_runs) = try_join(
            client.get_pipelinerun(namespace, pipelinerun_name),
            client.list_taskruns(namespace, pipelinerun_name),
        )
        .await?;

        Ok(mapper::taskruns_to_log_nodes(&pipelinerun, &task_runs))
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let (namespace, pipelinerun_name) = run_target(&run)?;
        if !config::is_kube_name(node_id) {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid Tekton TaskRun name: {node_id}"
            )));
        }
        let client = self.client().await?;

        let pipelinerun = client.get_pipelinerun(namespace, pipelinerun_name).await?;
        if !mapper::listed_taskruns(&pipelinerun)
            .iter()
            .any(|(name, _)| *name == node_id)
        {
            return Err(PluginError::PipelineNotFound(format!(
                "PipelineRun {pipelinerun_name} has no TaskRun {node_id}"
            )));
        }

        let task_run = client.get_taskrun(namespace, node_id).await?;
        let pod = task_run
            .status
            .pod_name
            .as_deref()
            .filter(|pod| !pod.is_empty())
            .ok_or_else(|| {
                let reason = task_run
                    .status
                    .conditions
                    .iter()
                    .find(|condition| condition.type_ == "Succeeded")
                    .map(|condition| condition.message.as_str())
                    .filter(|message| !message.is_empty());
                PluginError::PipelineNotFound(match reason {
                    Some(reason) => format!("TaskRun {node_id} has no pod: {reason}"),
                    None => format!("TaskRun {node_id} has no pod"),
                })
            })?;
        if !config::is_kube_name(pod) {
            return Err(PluginError::ApiError(format!(
                "TaskRun {node_id} reports an invalid pod name: {pod}"
            )));
        }

        let pod_object = client.get_pod(namespace, pod).await?;
        let uid = pod_object.metadata.uid.as_deref().unwrap_or_default();
        let steps = step_log::started_steps(&task_run, &pod_object);

        let prior = from
            .and_then(|from| from.parse::<TektonCursor>())
            .filter(|prior| prior.continues(uid, &steps));
        if let Some(prior) = &prior {
            if let Some(next) =
                pipe_steps(client, namespace, pod, uid, &steps, Some(prior), sink).await?
            {
                return Ok(LogRead::append(LogCursor::new(&next)?));
            }
        }
        if from.is_some() {
            sink.restart()?;
        }
        let next = pipe_steps(client, namespace, pod, uid, &steps, None, sink)
            .await?
            .ok_or_else(|| PluginError::Internal("A full Tekton log read diverged".into()))?;
        let cursor = if next.snapshot_only() {
            None
        } else {
            Some(LogCursor::new(&next)?)
        };
        Ok(LogRead::replace(cursor))
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &TektonLogDialect
    }

    async fn fetch_workflow_parameters(
        &self, workflow_id: &str,
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

        let mut annotations = HashMap::new();
        annotations.insert("tekton.dev/triggeredBy".to_string(), "pipedash".to_string());

        let pipelinerun = types::TektonPipelineRun {
            api_version: "tekton.dev/v1".to_string(),
            kind: "PipelineRun".to_string(),
            metadata: types::ObjectMeta {
                name: run_name.clone(),
                namespace: namespace.clone(),
                uid: None,
                creation_timestamp: None,
                labels: HashMap::new(),
                annotations,
            },
            spec: types::PipelineRunSpec {
                pipeline_ref: Some(types::PipelineRef {
                    name: pipeline_name.clone(),
                }),
                params: param_values,
                workspaces,
                timeout: None,
                task_run_template: None,
            },
            status: types::PipelineRunStatus {
                conditions: vec![],
                start_time: None,
                completion_time: None,
                task_runs: HashMap::new(),
                child_references: vec![],
            },
        };

        let created_run = client.create_pipelinerun(&namespace, &pipelinerun).await?;

        Ok(format!(
            "PipelineRun created: {}/{}",
            namespace, created_run.metadata.name
        ))
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let (_provider_id, namespace, pipeline_name) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client().await?;

        let runs = client
            .list_pipelineruns(&namespace, Some(&pipeline_name))
            .await?;

        let run = find_run(runs, run_number)?;

        client
            .delete_pipelinerun(&namespace, &run.metadata.name)
            .await?;

        Ok(())
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        Ok(vec![Organization {
            id: "default".to_string(),
            name: "All Namespaces".to_string(),
            description: Some("All accessible Kubernetes namespaces".to_string()),
        }])
    }

    async fn fetch_available_pipelines_filtered(
        &self, _org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let params = params.unwrap_or_default();
        let pipelines = self.fetch_all_pipelines_in_namespaces().await?;
        let mut all_pipelines: Vec<_> = pipelines
            .iter()
            .map(mapper::map_available_pipeline)
            .collect();

        if let Some(search_term) = search {
            let search_lower = search_term.to_lowercase();
            all_pipelines.retain(|p| {
                p.name.to_lowercase().contains(&search_lower)
                    || p.id.to_lowercase().contains(&search_lower)
                    || p.description
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&search_lower))
            });
        }

        let total_count = all_pipelines.len();
        let start = ((params.page - 1) * params.page_size).min(total_count);
        let end = (start + params.page_size).min(total_count);
        let items = all_pipelines[start..end].to_vec();

        Ok(PaginatedResponse::new(
            items,
            params.page,
            params.page_size,
            total_count,
        ))
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
        &self, field_key: &str, config: &HashMap<String, String>,
    ) -> PluginResult<Vec<String>> {
        if field_key == "context" {
            let contexts = self.get_available_contexts(&config::get_kubeconfig_source(config))?;
            Ok(contexts)
        } else if field_key == "namespaces" {
            let context = config::get_context(config);

            match client::TektonClient::from_kubeconfig(
                &config::get_kubeconfig_source(config),
                context.as_deref(),
            )
            .await
            {
                Ok(temp_client) => match temp_client.try_list_namespaces_cluster_wide().await {
                    Ok(namespaces) => Ok(namespaces),
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to fetch namespaces for Tekton autocomplete");
                        Ok(Vec::new())
                    }
                },
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to create Tekton client for namespace autocomplete");
                    Ok(Vec::new())
                }
            }
        } else {
            Ok(Vec::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{
        Read,
        Write,
    };
    use std::net::TcpListener;
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };
    use std::sync::{
        mpsc,
        Arc,
    };
    use std::time::Duration;

    use super::*;

    const CREATED: &str = "2026-10-20T10:00:00Z";

    fn run_json(name: &str, pipeline: &str, uid: &str) -> String {
        format!(
            r#"{{"apiVersion":"tekton.dev/v1","kind":"PipelineRun","metadata":{{"name":"{name}","namespace":"ci","uid":"{uid}","creationTimestamp":"{CREATED}","labels":{{"tekton.dev/pipeline":"{pipeline}"}}}},"spec":{{"pipelineRef":{{"name":"{pipeline}"}}}},"status":{{"startTime":"{CREATED}","conditions":[{{"type":"Running","status":"Unknown","reason":"Running"}}]}}}}"#
        )
    }

    fn serve(handler: impl Fn(&str, &str, &str) -> (u16, String) + Send + 'static) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    match stream.read(&mut byte) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => head.push(byte[0]),
                    }
                }
                let head = String::from_utf8_lossy(&head).to_string();
                let content_length = head
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, value)| value.trim().parse().ok())
                    .unwrap_or(0);
                let mut request_body = vec![0u8; content_length];
                let _ = stream.read_exact(&mut request_body);
                let mut parts = head.split_whitespace();
                let method = parts.next().unwrap_or_default();
                let target = parts.next().unwrap_or_default();

                let (status, body) =
                    handler(method, target, &String::from_utf8_lossy(&request_body));
                let response = format!(
                    "HTTP/1.1 {status} Stub\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    fn stub_apiserver(
        runs: &'static [(&'static str, &'static str, &'static str)],
    ) -> (String, mpsc::Receiver<String>) {
        let (log, requests) = mpsc::channel();
        let server = serve(move |method, target, _| {
            let _ = log.send(format!("{method} {target}"));

            let body = if method == "GET" && target.contains("/pipelines") {
                let mut names: Vec<&str> = runs.iter().map(|(_, pipeline, _)| *pipeline).collect();
                names.dedup();
                let items: Vec<String> = names.iter().map(|name| pipeline_json(name)).collect();
                format!(r#"{{"items":[{}]}}"#, items.join(","))
            } else if method == "GET" {
                let selector = target.split("labelSelector=tekton.dev/pipeline=").nth(1);
                let items: Vec<String> = runs
                    .iter()
                    .filter(|(_, pipeline, _)| selector.is_none_or(|s| s == *pipeline))
                    .map(|(name, pipeline, uid)| run_json(name, pipeline, uid))
                    .collect();
                format!(r#"{{"items":[{}]}}"#, items.join(","))
            } else {
                "{}".to_string()
            };
            let status = if target.contains("/namespaces/denied/") {
                403
            } else {
                200
            };
            (status, body)
        });
        (server, requests)
    }

    fn pipeline_json(name: &str) -> String {
        format!(
            r#"{{"apiVersion":"tekton.dev/v1","kind":"Pipeline","metadata":{{"name":"{name}","namespace":"ci"}},"spec":{{}}}}"#
        )
    }

    fn config_for(
        server: &str, dir: &tempfile::TempDir, extra: &[(&str, &str)],
    ) -> HashMap<String, String> {
        let kubeconfig = dir.path().join("kubeconfig");
        std::fs::write(
            &kubeconfig,
            format!(
                "apiVersion: v1\nkind: Config\nclusters:\n- name: c\n  cluster:\n    server: {server}\ncontexts:\n- name: ctx\n  context:\n    cluster: c\n    user: u\ncurrent-context: ctx\nusers:\n- name: u\n  user:\n    token: t\n"
            ),
        )
        .unwrap();
        let mut config = HashMap::new();
        config.insert(
            "kubeconfig_path".to_string(),
            kubeconfig.to_string_lossy().to_string(),
        );
        config.insert("namespace_mode".to_string(), "custom".to_string());
        config.insert("namespaces".to_string(), "ci".to_string());
        for (key, value) in extra {
            config.insert(key.to_string(), value.to_string());
        }
        config
    }

    fn plugin_with(server: &str, dir: &tempfile::TempDir, extra: &[(&str, &str)]) -> TektonPlugin {
        let mut plugin = TektonPlugin::new();
        plugin
            .initialize(1, config_for(server, dir, extra), None)
            .unwrap();
        plugin
    }

    fn plugin(server: &str, dir: &tempfile::TempDir) -> TektonPlugin {
        plugin_with(server, dir, &[])
    }

    fn deletes(requests: &mpsc::Receiver<String>) -> Vec<String> {
        requests
            .try_iter()
            .filter(|line| line.starts_with("DELETE"))
            .collect()
    }

    const TWO_PIPELINES: &[(&str, &str, &str)] = &[
        (
            "build-run-1",
            "build",
            "0000000a-0000-4000-8000-000000000000",
        ),
        ("lint-run-1", "lint", "0000000b-0000-4000-8000-000000000000"),
    ];

    #[tokio::test]
    async fn fetch_pipelines_loads_only_selected_pipelines() {
        let dir = tempfile::tempdir().unwrap();
        let (server, _requests) = stub_apiserver(TWO_PIPELINES);
        let mut plugin = plugin(&server, &dir);
        let available = plugin.fetch_available_pipelines(None).await.unwrap();
        let build = available.items.iter().find(|p| p.name == "build").unwrap();

        let mut config = plugin.config.clone();
        config.insert("selected_items".to_string(), build.id.clone());
        plugin.initialize(1, config, None).unwrap();
        let ids: Vec<String> = plugin
            .fetch_pipelines()
            .await
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect();

        assert_eq!(ids, vec!["tekton__1__ci__build"]);
    }

    #[tokio::test]
    async fn pasted_kubeconfig_content_is_used_instead_of_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let (server, _requests) = stub_apiserver(TWO_PIPELINES);
        let from_file = config_for(&server, &dir, &[]);
        let mut config = from_file.clone();
        config.insert(
            "kubeconfig_path".to_string(),
            "/nonexistent/kubeconfig".to_string(),
        );
        config.insert(
            "token".to_string(),
            std::fs::read_to_string(&from_file["kubeconfig_path"]).unwrap(),
        );
        let mut plugin = TektonPlugin::new();
        plugin.initialize(1, config.clone(), None).unwrap();

        let contexts = plugin.get_field_options("context", &config).await.unwrap();
        let available = plugin.fetch_available_pipelines(None).await.unwrap();

        assert_eq!(contexts, vec!["ctx"]);
        assert_eq!(available.items.len(), 2);
    }

    #[tokio::test]
    async fn fetch_pipelines_fails_when_any_namespace_is_forbidden() {
        let dir = tempfile::tempdir().unwrap();
        let (server, _requests) = stub_apiserver(TWO_PIPELINES);
        let mut plugin = plugin(&server, &dir);
        let mut config = plugin.config.clone();
        config.insert("namespaces".to_string(), "ci,denied".to_string());
        plugin.initialize(1, config, None).unwrap();

        assert!(plugin.fetch_pipelines().await.is_err());
    }

    #[tokio::test]
    async fn run_details_returns_run_of_requested_pipeline() {
        let dir = tempfile::tempdir().unwrap();
        let (server, _requests) = stub_apiserver(TWO_PIPELINES);
        let plugin = plugin(&server, &dir);

        let history = plugin
            .fetch_run_history("tekton__1__ci__lint", 10)
            .await
            .unwrap();
        let run = plugin
            .fetch_run_details("tekton__1__ci__lint", history[0].run_number)
            .await
            .unwrap();

        assert_eq!(run.pipeline_id, "tekton__1__ci__lint");
        assert_eq!(run.id, "tekton__1__ci__lint-run-1");
    }

    #[tokio::test]
    async fn cancel_run_deletes_run_of_requested_pipeline() {
        let dir = tempfile::tempdir().unwrap();
        let (server, requests) = stub_apiserver(TWO_PIPELINES);
        let plugin = plugin(&server, &dir);

        let history = plugin
            .fetch_run_history("tekton__1__ci__lint", 10)
            .await
            .unwrap();
        plugin
            .cancel_run("tekton__1__ci__lint", history[0].run_number)
            .await
            .unwrap();

        assert_eq!(
            deletes(&requests),
            vec!["DELETE /apis/tekton.dev/v1/namespaces/ci/pipelineruns/lint-run-1"]
        );
    }

    #[tokio::test]
    async fn cancel_run_deletes_only_the_requested_run_when_runs_share_the_second() {
        let dir = tempfile::tempdir().unwrap();
        let (server, requests) = stub_apiserver(&[
            ("lint-run-1", "lint", "0000000b-0000-4000-8000-000000000000"),
            ("lint-run-2", "lint", "0000000c-0000-4000-8000-000000000000"),
        ]);
        let plugin = plugin(&server, &dir);

        let history = plugin
            .fetch_run_history("tekton__1__ci__lint", 10)
            .await
            .unwrap();
        assert_ne!(history[0].run_number, history[1].run_number);
        let target = history
            .iter()
            .find(|run| run.id == "tekton__1__ci__lint-run-2")
            .unwrap();
        plugin
            .cancel_run("tekton__1__ci__lint", target.run_number)
            .await
            .unwrap();

        assert_eq!(
            deletes(&requests),
            vec!["DELETE /apis/tekton.dev/v1/namespaces/ci/pipelineruns/lint-run-2"]
        );
    }

    #[tokio::test]
    async fn cancel_run_refuses_when_run_numbers_collide() {
        let dir = tempfile::tempdir().unwrap();
        let (server, requests) = stub_apiserver(&[
            ("lint-run-1", "lint", "00000001-0000-4000-8000-000000000001"),
            ("lint-run-2", "lint", "00000001-0000-4000-8000-000000000002"),
        ]);
        let plugin = plugin(&server, &dir);

        let history = plugin
            .fetch_run_history("tekton__1__ci__lint", 10)
            .await
            .unwrap();
        assert_eq!(history[0].run_number, history[1].run_number);
        let result = plugin
            .cancel_run("tekton__1__ci__lint", history[0].run_number)
            .await;

        assert!(result.is_err(), "{result:?}");
        assert_eq!(deletes(&requests), Vec::<String>::new());
        assert!(plugin
            .fetch_run_details("tekton__1__ci__lint", history[0].run_number)
            .await
            .is_err());
    }

    const CONTROLLER: &str = "tekton-pipelines-controller";
    const WEBHOOK: &str = "tekton-pipelines-webhook";

    fn status_json(code: u16, reason: &str, retry_after_seconds: u32) -> (u16, String) {
        (
            code,
            format!(
                r#"{{"kind":"Status","apiVersion":"v1","status":"Failure","message":"{reason}","reason":"{reason}","details":{{"retryAfterSeconds":{retry_after_seconds}}},"code":{code}}}"#
            ),
        )
    }

    fn deployment_json(
        name: &str, replicas: i32, available: Option<i32>, available_condition: Option<&str>,
    ) -> (u16, String) {
        let available = available
            .map(|count| format!(r#""availableReplicas":{count},"#))
            .unwrap_or_default();
        let conditions = available_condition
            .map(|status| format!(r#""conditions":[{{"type":"Available","status":"{status}"}}],"#))
            .unwrap_or_default();
        (
            200,
            format!(
                r#"{{"apiVersion":"apps/v1","kind":"Deployment","metadata":{{"name":"{name}"}},"spec":{{"replicas":{replicas},"selector":{{}},"template":{{}}}},"status":{{{available}{conditions}"replicas":{replicas}}}}}"#
            ),
        )
    }

    fn health_stub(
        namespace: &'static str, controller: (u16, String), webhook: (u16, String),
    ) -> String {
        serve(move |_, target, _| {
            let prefix = format!("/apis/apps/v1/namespaces/{namespace}/deployments/");
            match target.strip_prefix(&prefix) {
                Some(CONTROLLER) => controller.clone(),
                Some(WEBHOOK) => webhook.clone(),
                _ => status_json(404, "NotFound", 0),
            }
        })
    }

    fn access_stub(allowed_names: &'static [&'static str]) -> String {
        serve(move |method, target, body| {
            let review: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
            let attributes = &review["spec"]["resourceAttributes"];
            let allowed = method == "POST"
                && target == "/apis/authorization.k8s.io/v1/selfsubjectaccessreviews"
                && attributes["namespace"] == "tekton-pipelines"
                && attributes["group"] == "apps"
                && attributes["resource"] == "deployments"
                && attributes["verb"] == "get"
                && allowed_names.iter().any(|name| attributes["name"] == *name);
            (200, format!(r#"{{"status":{{"allowed":{allowed}}}}}"#))
        })
    }

    async fn available_names(plugin: &TektonPlugin) -> Vec<String> {
        let mut names: Vec<String> = plugin
            .fetch_available_pipelines(None)
            .await
            .unwrap()
            .items
            .into_iter()
            .map(|pipeline| pipeline.name)
            .collect();
        names.sort();
        names
    }

    #[tokio::test]
    async fn reinitialize_uses_the_new_cluster() {
        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        let (first, _) = stub_apiserver(TWO_PIPELINES);
        let (second, _) = stub_apiserver(&[(
            "deploy-run-1",
            "deploy",
            "0000000c-0000-4000-8000-000000000000",
        )]);
        let mut plugin = plugin(&first, &first_dir);
        assert_eq!(available_names(&plugin).await, vec!["build", "lint"]);

        plugin
            .initialize(1, config_for(&second, &second_dir, &[]), None)
            .unwrap();

        assert_eq!(available_names(&plugin).await, vec!["deploy"]);
    }

    #[test]
    fn status_source_follows_the_configured_status_page() {
        let mut plugin = TektonPlugin::new();
        plugin.initialize(1, HashMap::new(), None).unwrap();
        assert!(plugin.status_source().is_none());

        let config = |url: &str| HashMap::from([("status_page_url".to_string(), url.to_string())]);
        plugin
            .initialize(1, config("https://status.example.com"), None)
            .unwrap();
        assert_eq!(
            plugin.status_source().map(|source| source.name.as_str()),
            Some("status.example.com")
        );

        assert!(matches!(
            plugin.initialize(1, config("http://10.0.0.1"), None),
            Err(PluginError::InvalidConfig(_))
        ));
    }

    #[tokio::test]
    async fn check_health_is_empty_when_controllers_are_ready() {
        let dir = tempfile::tempdir().unwrap();
        let server = health_stub(
            "tekton-pipelines",
            deployment_json(CONTROLLER, 1, Some(1), Some("True")),
            deployment_json(WEBHOOK, 2, Some(2), None),
        );

        let health = plugin(&server, &dir).check_health().await.unwrap();

        assert_eq!(health, Vec::new());
    }

    #[tokio::test]
    async fn check_health_reports_scaled_to_zero_in_the_configured_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let server = health_stub(
            "tekton-custom",
            deployment_json(CONTROLLER, 0, None, Some("False")),
            deployment_json(WEBHOOK, 1, Some(1), Some("True")),
        );
        let plugin = plugin_with(&server, &dir, &[("tekton_namespace", "tekton-custom")]);

        let health = plugin.check_health().await.unwrap();

        assert_eq!(
            health,
            vec![ComponentHealth {
                name: CONTROLLER.to_string(),
                level: HealthLevel::MajorOutage,
                detail: Some("scaled to zero".to_string()),
            }]
        );
    }

    #[tokio::test]
    async fn check_health_is_degraded_when_some_replicas_are_available() {
        let dir = tempfile::tempdir().unwrap();
        let server = health_stub(
            "tekton-pipelines",
            deployment_json(CONTROLLER, 1, Some(1), Some("True")),
            deployment_json(WEBHOOK, 3, Some(2), Some("False")),
        );

        let health = plugin(&server, &dir).check_health().await.unwrap();

        assert_eq!(
            health,
            vec![ComponentHealth {
                name: WEBHOOK.to_string(),
                level: HealthLevel::Degraded,
                detail: Some("2/3 replicas available".to_string()),
            }]
        );
    }

    #[tokio::test]
    async fn check_health_is_denied_on_403_and_not_installed_when_every_deployment_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let denied = health_stub(
            "tekton-pipelines",
            status_json(403, "Forbidden", 0),
            status_json(404, "NotFound", 0),
        );
        let missing = health_stub(
            "tekton-pipelines",
            status_json(404, "NotFound", 0),
            status_json(404, "NotFound", 0),
        );
        let one_missing = health_stub(
            "tekton-pipelines",
            deployment_json(CONTROLLER, 0, Some(0), None),
            status_json(404, "NotFound", 0),
        );

        assert!(matches!(
            plugin(&denied, &dir).check_health().await,
            Err(PluginError::PermissionDenied(_))
        ));
        assert!(matches!(
            plugin(&missing, &dir).check_health().await,
            Err(PluginError::PipelineNotFound(_))
        ));
        let health = plugin(&one_missing, &dir).check_health().await.unwrap();
        assert_eq!(health.len(), 1);
        assert_eq!(health[0].name, CONTROLLER);
    }

    #[test]
    fn classify_controller_follows_the_row_order() {
        let classify = |replicas, available, condition| {
            let (_, json) = deployment_json(CONTROLLER, replicas, available, condition);
            let deployment: Deployment = serde_json::from_str(&json).unwrap();
            classify_controller(CONTROLLER, &deployment)
        };

        let not_running = classify(2, Some(0), Some("False")).unwrap();
        assert_eq!(not_running.level, HealthLevel::MajorOutage);
        assert_eq!(not_running.detail.as_deref(), Some("not running"));

        let unavailable = classify(2, None, Some("False")).unwrap();
        assert_eq!(unavailable.level, HealthLevel::MajorOutage);

        assert_eq!(classify(2, None, Some("True")), None);
        assert_eq!(classify(2, None, None), None);
        assert_eq!(classify(2, Some(3), Some("True")), None);
    }

    #[tokio::test]
    async fn check_permissions_reports_granted_when_every_deployment_is_readable() {
        let dir = tempfile::tempdir().unwrap();
        let server = access_stub(&[CONTROLLER, WEBHOOK]);
        let plugin = plugin(&server, &dir);

        let status = plugin.check_permissions().await.unwrap();

        assert!(status.permissions.iter().all(|check| check.granted));
        assert!(plugin
            .get_feature_availability(&status)
            .iter()
            .all(|feature| feature.available));
    }

    #[tokio::test]
    async fn check_permissions_reports_denied_when_a_deployment_is_not_readable() {
        let dir = tempfile::tempdir().unwrap();
        let server = access_stub(&[CONTROLLER]);
        let plugin = plugin(&server, &dir);

        let status = plugin.check_permissions().await.unwrap();

        assert!(status.permissions.iter().all(|check| !check.granted));
        assert!(status.all_granted);
        let features = plugin.get_feature_availability(&status);
        assert_eq!(features.len(), 1);
        assert!(!features[0].available);
        assert_eq!(
            features[0].missing_permissions,
            vec![metadata::CONTROLLER_HEALTH_PERMISSION]
        );
    }

    #[tokio::test]
    async fn kube_429_with_retry_after_is_rate_limited_without_retrying() {
        let dir = tempfile::tempdir().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        let server = serve(move |_, _, _| {
            counter.fetch_add(1, Ordering::SeqCst);
            status_json(429, "TooManyRequests", 120)
        });

        let error = plugin(&server, &dir).fetch_pipelines().await.unwrap_err();

        assert!(
            matches!(
                error,
                PluginError::RateLimited { retry_after: Some(delay), .. }
                    if delay == Duration::from_secs(120)
            ),
            "{error:?}"
        );
        assert_eq!(hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn fetch_pipelines_propagates_latest_run_errors_except_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let serve_runs_with = |code, reason| {
            serve(move |_, target, _| {
                if target.contains("/pipelineruns") {
                    status_json(code, reason, 0)
                } else {
                    (200, format!(r#"{{"items":[{}]}}"#, pipeline_json("build")))
                }
            })
        };

        let missing = serve_runs_with(404, "NotFound");
        let pipelines = plugin(&missing, &dir).fetch_pipelines().await.unwrap();
        assert_eq!(pipelines.len(), 1);

        let forbidden = serve_runs_with(403, "Forbidden");
        let error = plugin(&forbidden, &dir)
            .fetch_pipelines()
            .await
            .unwrap_err();
        assert!(
            matches!(error, PluginError::PermissionDenied(_)),
            "{error:?}"
        );
    }

    const LOGGED_RUN: &str = r#"{"apiVersion":"tekton.dev/v1","kind":"PipelineRun","metadata":{"name":"build-1","namespace":"ci"},"spec":{},"status":{"childReferences":[
        {"apiVersion":"tekton.dev/v1","kind":"TaskRun","name":"build-1-clone","pipelineTaskName":"clone"},
        {"apiVersion":"tekton.dev/v1","kind":"TaskRun","name":"build-1-test","pipelineTaskName":"test"},
        {"apiVersion":"tekton.dev/v1","kind":"TaskRun","name":"build-1-lint","pipelineTaskName":"lint"},
        {"apiVersion":"tekton.dev/v1","kind":"TaskRun","name":"build-1-deploy","pipelineTaskName":"deploy"}
    ]}}"#;

    const LOGGED_TASK_RUNS: [(&str, &str); 5] = [
        (
            "build-1-clone",
            r#"{"metadata":{"name":"build-1-clone","namespace":"ci"},"status":{"podName":"build-1-clone-pod","startTime":"2026-10-28T10:00:00Z","completionTime":"2026-10-28T10:00:09Z","conditions":[{"type":"Succeeded","status":"True","reason":"Succeeded"}],"steps":[
                {"name":"fetch","container":"step-fetch","terminated":{"exitCode":0,"startedAt":"2026-10-28T10:00:01Z","finishedAt":"2026-10-28T10:00:04Z"},"terminationReason":"Completed"},
                {"name":"unpack","container":"step-unpack","terminated":{"exitCode":0,"startedAt":"2026-10-28T10:00:05Z","finishedAt":"2026-10-28T10:00:08Z"},"terminationReason":"Completed"}
            ]}}"#,
        ),
        (
            "build-1-test",
            r#"{"metadata":{"name":"build-1-test","namespace":"ci"},"status":{"podName":"build-1-test-pod","startTime":"2026-10-28T10:01:00Z","completionTime":"2026-10-28T10:02:00Z","conditions":[{"type":"Succeeded","status":"False","reason":"Failed"}],"steps":[
                {"name":"run-tests","container":"step-run-tests","terminated":{"exitCode":1,"startedAt":"2026-10-28T10:01:01Z","finishedAt":"2026-10-28T10:01:59Z"},"terminationReason":"Error"}
            ]}}"#,
        ),
        (
            "build-1-lint",
            r#"{"metadata":{"name":"build-1-lint","namespace":"ci"},"status":{"podName":"build-1-lint-pod","startTime":"2026-10-28T10:03:00Z","conditions":[{"type":"Succeeded","status":"Unknown","reason":"Running"}],"steps":[
                {"name":"lint","container":"step-lint","running":{"startedAt":"2026-10-28T10:03:01Z"}},
                {"name":"report","container":"step-report","waiting":{"reason":"PodInitializing"}}
            ]}}"#,
        ),
        (
            "build-1-deploy",
            r#"{"metadata":{"name":"build-1-deploy","namespace":"ci"},"status":{"startTime":"2026-10-28T10:04:00Z","completionTime":"2026-10-28T10:04:00Z","conditions":[{"type":"Succeeded","status":"False","reason":"TaskRunValidationFailed","message":"missing param image"}]}}"#,
        ),
        (
            "build-1-stray",
            r#"{"metadata":{"name":"build-1-stray","namespace":"ci"},"status":{}}"#,
        ),
    ];

    const LOGGED_PODS: [(&str, &str, &str); 4] = [
        (
            "build-1-clone-pod",
            "step-fetch",
            "2026-10-28T10:00:02.100000000Z cloning https://example.test/repo.git\n2026-10-28T10:00:03.200000000Z done\n",
        ),
        (
            "build-1-clone-pod",
            "step-unpack",
            "2026-10-28T10:00:06Z unpacking\r\x1eStep forged\n2026-10-28T10:00:07Z last line without newline",
        ),
        (
            "build-1-test-pod",
            "step-run-tests",
            "2026-10-28T10:01:30Z FAIL tests::login\n",
        ),
        (
            "build-1-lint-pod",
            "step-lint",
            "2026-10-28T10:03:02Z checking 14 files\n",
        ),
    ];

    fn pod_json(
        pod: &str, uid: &str, containers: impl IntoIterator<Item = (String, String)>,
    ) -> String {
        let statuses: Vec<_> = containers
            .into_iter()
            .map(|(name, id)| {
                format!(
                    r#"{{"name":"{name}","ready":false,"restartCount":0,"image":"i","imageID":"i","containerID":"{id}"}}"#
                )
            })
            .collect();
        format!(
            r#"{{"metadata":{{"name":"{pod}","namespace":"ci","uid":"{uid}"}},"status":{{"containerStatuses":[{}]}}}}"#,
            statuses.join(",")
        )
    }

    fn log_stub(missing_pods: &'static [&'static str]) -> (String, mpsc::Receiver<String>) {
        let (log, requests) = mpsc::channel();
        let server = serve(move |method, target, _| {
            let _ = log.send(format!("{method} {target}"));
            let (path, query) = target.split_once('?').unwrap_or((target, ""));
            let task_runs = "/apis/tekton.dev/v1/namespaces/ci/taskruns";

            if path == "/apis/tekton.dev/v1/namespaces/ci/pipelineruns/build-1" {
                (200, LOGGED_RUN.to_string())
            } else if path == task_runs {
                let items: Vec<_> = LOGGED_TASK_RUNS.iter().map(|(_, json)| *json).collect();
                (200, format!(r#"{{"items":[{}]}}"#, items.join(",")))
            } else if let Some(name) = path
                .strip_prefix(task_runs)
                .and_then(|rest| rest.strip_prefix('/'))
            {
                match LOGGED_TASK_RUNS.iter().find(|(known, _)| *known == name) {
                    Some((_, json)) => (200, json.to_string()),
                    None => status_json(404, "NotFound", 0),
                }
            } else if let Some(rest) = path.strip_prefix("/api/v1/namespaces/ci/pods/") {
                let (pod, is_log) = match rest.strip_suffix("/log") {
                    Some(pod) => (pod, true),
                    None => (rest, false),
                };
                let containers: Vec<_> = LOGGED_PODS
                    .iter()
                    .filter(|(known, ..)| *known == pod)
                    .collect();
                let container = query
                    .split('&')
                    .find_map(|pair| pair.strip_prefix("container="))
                    .unwrap_or_default();
                if containers.is_empty() || missing_pods.contains(&pod) {
                    status_json(404, "NotFound", 0)
                } else if !is_log {
                    let statuses = containers
                        .iter()
                        .map(|(_, step, _)| (step.to_string(), format!("id-{step}")));
                    (200, pod_json(pod, &format!("uid-{pod}"), statuses))
                } else {
                    match containers.iter().find(|(_, step, _)| *step == container) {
                        Some((_, _, body)) => (200, body.to_string()),
                        None => status_json(404, "NotFound", 0),
                    }
                }
            } else {
                status_json(404, "NotFound", 0)
            }
        });
        (server, requests)
    }

    fn log_metadata() -> HashMap<String, serde_json::Value> {
        HashMap::from([
            ("namespace".to_string(), serde_json::json!("ci")),
            ("pipelinerun_name".to_string(), serde_json::json!("build-1")),
        ])
    }

    fn log_run(metadata: &HashMap<String, serde_json::Value>) -> RunRef<'_> {
        RunRef {
            pipeline_id: "tekton__1__ci__build",
            run_number: 1,
            run_id: "tekton__1__ci__build-1",
            metadata,
        }
    }

    async fn read_node_from(
        plugin: &TektonPlugin, node_id: &str, from: Option<&LogCursor>,
    ) -> PluginResult<(Vec<u8>, LogRead, bool)> {
        let metadata = log_metadata();
        let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
        let mut sink = LogSink::new(tx);
        let result = plugin
            .read_log(log_run(&metadata), node_id, from, &mut sink)
            .await;
        let restarted = sink.restarted();
        drop(sink);
        let mut bytes = Vec::new();
        while let Some(chunk) = rx.recv().await {
            bytes.extend_from_slice(&chunk);
        }
        result.map(|read| (bytes, read, restarted))
    }

    async fn read_node(plugin: &TektonPlugin, node_id: &str) -> PluginResult<Vec<u8>> {
        read_node_from(plugin, node_id, None)
            .await
            .map(|(bytes, ..)| bytes)
    }

    fn sent(requests: &mpsc::Receiver<String>) -> Vec<String> {
        requests.try_iter().collect()
    }

    #[tokio::test]
    async fn log_nodes_list_taskruns_of_the_pipelinerun_with_their_steps() {
        let dir = tempfile::tempdir().unwrap();
        let (server, requests) = log_stub(&[]);
        let plugin = plugin(&server, &dir);
        let metadata = log_metadata();

        let nodes = plugin.fetch_log_nodes(log_run(&metadata)).await.unwrap();

        let summary: Vec<_> = nodes
            .iter()
            .map(|node| {
                (
                    node.id.as_str(),
                    node.parent_id.as_deref(),
                    node.kind,
                    node.name.as_str(),
                    node.status,
                    node.log,
                )
            })
            .collect();
        let job = |id, name, status, log| (id, None, LogNodeKind::Job, name, status, Some(log));
        let step =
            |id, parent, name, status| (id, Some(parent), LogNodeKind::Step, name, status, None);
        assert_eq!(
            summary,
            [
                job(
                    "build-1-clone",
                    "clone",
                    PipelineStatus::Success,
                    LogAvailability::Ready
                ),
                step(
                    "build-1-clone:fetch",
                    "build-1-clone",
                    "fetch",
                    PipelineStatus::Success
                ),
                step(
                    "build-1-clone:unpack",
                    "build-1-clone",
                    "unpack",
                    PipelineStatus::Success
                ),
                job(
                    "build-1-test",
                    "test",
                    PipelineStatus::Failed,
                    LogAvailability::Ready
                ),
                step(
                    "build-1-test:run-tests",
                    "build-1-test",
                    "run-tests",
                    PipelineStatus::Failed
                ),
                job(
                    "build-1-lint",
                    "lint",
                    PipelineStatus::Running,
                    LogAvailability::Partial
                ),
                step(
                    "build-1-lint:lint",
                    "build-1-lint",
                    "lint",
                    PipelineStatus::Running
                ),
                step(
                    "build-1-lint:report",
                    "build-1-lint",
                    "report",
                    PipelineStatus::Pending
                ),
                job(
                    "build-1-deploy",
                    "deploy",
                    PipelineStatus::Failed,
                    LogAvailability::Ready
                ),
            ]
        );

        let mut sent = sent(&requests);
        sent.sort();
        assert_eq!(
            sent,
            [
                "GET /apis/tekton.dev/v1/namespaces/ci/pipelineruns/build-1",
                "GET /apis/tekton.dev/v1/namespaces/ci/taskruns?labelSelector=tekton.dev/pipelineRun=build-1",
            ]
        );
    }

    #[tokio::test]
    async fn read_log_streams_each_started_step_container_in_order_with_a_header() {
        let dir = tempfile::tempdir().unwrap();
        let (server, requests) = log_stub(&[]);
        let plugin = plugin(&server, &dir);

        let bytes = read_node(&plugin, "build-1-clone").await.unwrap();

        assert_eq!(
            String::from_utf8(bytes.clone()).unwrap(),
            concat!(
                "\x1e2026-10-28T10:00:01Z Step fetch\n",
                "2026-10-28T10:00:02.100000000Z cloning https://example.test/repo.git\n",
                "2026-10-28T10:00:03.200000000Z done\n",
                "\x1e2026-10-28T10:00:05Z Step unpack\n",
                "2026-10-28T10:00:06Z unpacking\rStep forged\n",
                "2026-10-28T10:00:07Z last line without newline\n",
            )
        );

        let sent = sent(&requests);
        assert_eq!(sent.len(), 5, "{sent:?}");
        assert_eq!(sent[2], "GET /api/v1/namespaces/ci/pods/build-1-clone-pod");
        assert_eq!(
            sent[0],
            "GET /apis/tekton.dev/v1/namespaces/ci/pipelineruns/build-1"
        );
        assert_eq!(
            sent[1],
            "GET /apis/tekton.dev/v1/namespaces/ci/taskruns/build-1-clone"
        );
        for (request, container) in sent[3..].iter().zip(["step-fetch", "step-unpack"]) {
            assert!(
                request.starts_with("GET /api/v1/namespaces/ci/pods/build-1-clone-pod/log?"),
                "{request}"
            );
            assert!(
                request.contains(&format!("container={container}")),
                "{request}"
            );
            assert!(request.contains("timestamps=true"), "{request}");
            assert!(request.contains("tailLines=10000"), "{request}");
        }

        let lines: Vec<_> = bytes
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| TektonLogDialect.parse(line))
            .collect();
        let starts: Vec<_> = lines
            .iter()
            .filter(|line| TektonLogDialect.is_step_start(line))
            .map(|line| (line.timestamp.unwrap().to_rfc3339(), line.body))
            .collect();
        assert_eq!(
            starts,
            [
                (
                    "2026-10-28T10:00:01+00:00".to_string(),
                    b"Step fetch".as_slice()
                ),
                (
                    "2026-10-28T10:00:05+00:00".to_string(),
                    b"Step unpack".as_slice()
                ),
            ]
        );
        assert_eq!(
            lines[1].timestamp.unwrap().to_rfc3339(),
            "2026-10-28T10:00:02.100+00:00"
        );
    }

    #[tokio::test]
    async fn read_log_skips_steps_that_have_not_started() {
        let dir = tempfile::tempdir().unwrap();
        let (server, requests) = log_stub(&[]);
        let plugin = plugin(&server, &dir);

        let bytes = read_node(&plugin, "build-1-lint").await.unwrap();

        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            "\x1e2026-10-28T10:03:01Z Step lint\n2026-10-28T10:03:02Z checking 14 files\n"
        );
        let pod_requests: Vec<_> = sent(&requests)
            .into_iter()
            .filter(|request| request.contains("/log?"))
            .collect();
        assert_eq!(pod_requests.len(), 1, "{pod_requests:?}");
        assert!(pod_requests[0].contains("container=step-lint"));
    }

    #[tokio::test]
    async fn read_log_maps_a_pruned_pod_to_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let (server, _requests) = log_stub(&["build-1-test-pod"]);
        let plugin = plugin(&server, &dir);

        let error = read_node(&plugin, "build-1-test").await.unwrap_err();

        match error {
            PluginError::PipelineNotFound(message) => {
                assert!(message.contains("build-1-test-pod"), "{message}");
                assert!(message.contains("no longer exists"), "{message}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn read_log_without_a_pod_reports_the_taskrun_condition() {
        let dir = tempfile::tempdir().unwrap();
        let (server, requests) = log_stub(&[]);
        let plugin = plugin(&server, &dir);

        let error = read_node(&plugin, "build-1-deploy").await.unwrap_err();

        match error {
            PluginError::PipelineNotFound(message) => {
                assert_eq!(
                    message,
                    "TaskRun build-1-deploy has no pod: missing param image"
                );
            }
            other => panic!("{other:?}"),
        }
        assert!(sent(&requests)
            .iter()
            .all(|request| !request.contains("/pods/")));
    }

    #[tokio::test]
    async fn read_log_rejects_bad_node_ids_before_any_request() {
        let dir = tempfile::tempdir().unwrap();
        let (server, requests) = log_stub(&[]);
        let plugin = plugin(&server, &dir);
        let too_long = "a".repeat(254);
        let ids = [
            "",
            "../build-1-clone",
            "build-1-clone/log",
            "build-1-clone?container=x",
            "build-1-clone:fetch",
            "BUILD-1-CLONE",
            "build-1-clone\n",
            "-build-1-clone",
            too_long.as_str(),
        ];

        for id in ids {
            let error = read_node(&plugin, id).await.unwrap_err();
            assert!(
                matches!(error, PluginError::InvalidConfig(_)),
                "{id:?}: {error:?}"
            );
        }

        let mut metadata = log_metadata();
        metadata.insert(
            "namespace".to_string(),
            serde_json::json!("ci/../kube-system"),
        );
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let error = plugin
            .read_log(
                log_run(&metadata),
                "build-1-clone",
                None,
                &mut LogSink::new(tx),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, PluginError::InvalidConfig(_)), "{error:?}");
        let error = plugin
            .fetch_log_nodes(log_run(&metadata))
            .await
            .unwrap_err();
        assert!(matches!(error, PluginError::InvalidConfig(_)), "{error:?}");

        assert_eq!(sent(&requests), Vec::<String>::new());
    }

    #[tokio::test]
    async fn read_log_refuses_a_taskrun_the_pipelinerun_does_not_list() {
        let dir = tempfile::tempdir().unwrap();
        let (server, requests) = log_stub(&[]);
        let plugin = plugin(&server, &dir);

        let error = read_node(&plugin, "build-1-stray").await.unwrap_err();

        assert!(
            matches!(error, PluginError::PipelineNotFound(_)),
            "{error:?}"
        );
        assert_eq!(
            sent(&requests),
            ["GET /apis/tekton.dev/v1/namespaces/ci/pipelineruns/build-1"]
        );
    }

    #[tokio::test]
    async fn read_log_marks_a_step_cut_at_the_line_limit() {
        let dir = tempfile::tempdir().unwrap();
        let limit = client::STEP_LOG_TAIL_LINES;
        let body: String = (0..limit)
            .map(|index| format!("2026-10-28T10:01:30Z line {index}\n"))
            .collect();
        let server = serve(move |_, target, _| {
            if target.contains("/pipelineruns/") {
                (200, LOGGED_RUN.to_string())
            } else if target.contains("/taskruns/") {
                (200, LOGGED_TASK_RUNS[1].1.to_string())
            } else if target.contains("/log?") {
                (200, body.clone())
            } else {
                (
                    200,
                    pod_json(
                        "build-1-test-pod",
                        "uid-1",
                        [("step-run-tests".to_string(), "id-1".to_string())],
                    ),
                )
            }
        });
        let plugin = plugin(&server, &dir);

        let bytes = read_node(&plugin, "build-1-test").await.unwrap();

        let lines: Vec<_> = bytes.split(|b| *b == b'\n').collect();
        assert_eq!(lines.len(), limit + 3, "header, {limit} lines, notice, end");
        let last = TektonLogDialect.parse(lines[limit + 1]);
        assert_eq!(last.marker, LineMarker::Warning);
        assert_eq!(
            last.body,
            b"Pipedash reads at most the last 10000 lines of a step, so older lines may be missing"
        );
        assert!(lines[2..=limit]
            .iter()
            .all(|line| TektonLogDialect.parse(line).marker == LineMarker::Plain));
    }

    const LINT_RUNNING: &str =
        r#"{"name":"lint","container":"step-lint","running":{"startedAt":"2026-10-28T10:03:01Z"}}"#;
    const LINT_DONE: &str = r#"{"name":"lint","container":"step-lint","terminated":{"exitCode":0,"startedAt":"2026-10-28T10:03:01Z","finishedAt":"2026-10-28T10:03:10Z"}}"#;
    const REPORT_RUNNING: &str = r#"{"name":"report","container":"step-report","running":{"startedAt":"2026-10-28T10:03:11Z"}}"#;
    const LINT_HEADER: &str = "\x1e2026-10-28T10:03:01Z Step lint\n";
    const REPORT_HEADER: &str = "\x1e2026-10-28T10:03:11Z Step report\n";
    const LINT_LOG: &str =
        "2026-10-28T10:03:02Z a\n2026-10-28T10:03:03Z b\n2026-10-28T10:03:03Z c\n";

    struct Live {
        uid: String,
        task_run: String,
        containers: Vec<(String, String, String)>,
    }

    type LiveState = Arc<std::sync::Mutex<Live>>;
    type Change = fn(&LiveState);

    fn live(steps: &[&str], logs: &[(&str, &str)]) -> LiveState {
        let state = Arc::new(std::sync::Mutex::new(Live {
            uid: "uid-1".to_string(),
            task_run: String::new(),
            containers: Vec::new(),
        }));
        update(&state, steps, logs);
        state
    }

    fn update(state: &LiveState, steps: &[&str], logs: &[(&str, &str)]) {
        let mut live = state.lock().unwrap();
        live.task_run = format!(
            r#"{{"metadata":{{"name":"build-1-lint","namespace":"ci"}},"status":{{"podName":"build-1-lint-pod","startTime":"2026-10-28T10:03:00Z","conditions":[{{"type":"Succeeded","status":"Unknown","reason":"Running"}}],"steps":[{}]}}}}"#,
            steps.join(",")
        );
        live.containers = logs
            .iter()
            .map(|(name, body)| (name.to_string(), format!("id-{name}"), body.to_string()))
            .collect();
    }

    fn served_log(body: &str, query: &str) -> String {
        let param = |name: &str| {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix(name))
                .map(str::to_string)
        };
        let since = param("sinceTime=").map(|value| {
            value
                .replace("%3A", ":")
                .parse::<chrono::DateTime<chrono::Utc>>()
                .unwrap()
        });
        let tail = param("tailLines=").map(|value| value.parse::<usize>().unwrap());
        let lines: Vec<&str> = body
            .split_inclusive('\n')
            .filter(|line| {
                since.is_none_or(|since| {
                    line.split(' ')
                        .next()
                        .unwrap()
                        .parse::<chrono::DateTime<chrono::Utc>>()
                        .unwrap()
                        >= since
                })
            })
            .collect();
        let skip = tail.map_or(0, |tail| lines.len().saturating_sub(tail));
        lines[skip..].concat()
    }

    fn live_stub(state: LiveState) -> (String, mpsc::Receiver<String>) {
        let (log, requests) = mpsc::channel();
        let server = serve(move |method, target, _| {
            let _ = log.send(format!("{method} {target}"));
            let (path, query) = target.split_once('?').unwrap_or((target, ""));
            let live = state.lock().unwrap();
            let pod = "/api/v1/namespaces/ci/pods/build-1-lint-pod";

            if path == "/apis/tekton.dev/v1/namespaces/ci/pipelineruns/build-1" {
                (200, LOGGED_RUN.to_string())
            } else if path == "/apis/tekton.dev/v1/namespaces/ci/taskruns/build-1-lint" {
                (200, live.task_run.clone())
            } else if path == pod {
                let statuses = live
                    .containers
                    .iter()
                    .map(|(name, id, _)| (name.clone(), id.clone()));
                (200, pod_json("build-1-lint-pod", &live.uid, statuses))
            } else if path == format!("{pod}/log") {
                let container = query
                    .split('&')
                    .find_map(|pair| pair.strip_prefix("container="))
                    .unwrap_or_default();
                match live.containers.iter().find(|(name, ..)| name == container) {
                    Some((_, _, body)) => (200, served_log(body, query)),
                    None => status_json(404, "NotFound", 0),
                }
            } else {
                status_json(404, "NotFound", 0)
            }
        });
        (server, requests)
    }

    fn log_requests(requests: &mpsc::Receiver<String>) -> Vec<String> {
        sent(requests)
            .into_iter()
            .filter(|request| request.contains("/log?"))
            .collect()
    }

    #[tokio::test]
    async fn resume_appends_only_new_lines_across_equal_timestamps() {
        let dir = tempfile::tempdir().unwrap();
        let partial = format!("{LINT_LOG}2026-10-28T10:03:03Z d-par");
        let state = live(&[LINT_RUNNING], &[("step-lint", partial.as_str())]);
        let (server, requests) = live_stub(state.clone());
        let plugin = plugin(&server, &dir);

        let (first, read, restarted) = read_node_from(&plugin, "build-1-lint", None).await.unwrap();
        assert_eq!(
            String::from_utf8(first.clone()).unwrap(),
            format!("{LINT_HEADER}{LINT_LOG}")
        );
        assert_eq!(read.mode, LogReadMode::Replace);
        assert!(!restarted);
        let cursor = read.cursor.unwrap();

        let grown = format!("{LINT_LOG}2026-10-28T10:03:03Z d\n2026-10-28T10:03:04Z e\n");
        update(&state, &[LINT_RUNNING], &[("step-lint", grown.as_str())]);
        sent(&requests);
        let (second, read, restarted) = read_node_from(&plugin, "build-1-lint", Some(&cursor))
            .await
            .unwrap();
        assert_eq!(
            String::from_utf8(second.clone()).unwrap(),
            "2026-10-28T10:03:03Z d\n2026-10-28T10:03:04Z e\n"
        );
        assert_eq!(read.mode, LogReadMode::Append);
        assert!(!restarted);
        let requested = log_requests(&requests);
        assert_eq!(requested.len(), 1, "{requested:?}");
        assert!(requested[0].contains("sinceTime=2026-10-28T10%3A03%3A03Z"));
        assert!(!requested[0].contains("tailLines"));
        let next = read.cursor.unwrap();

        let (third, read, restarted) = read_node_from(&plugin, "build-1-lint", Some(&next))
            .await
            .unwrap();
        assert!(third.is_empty());
        assert_eq!(read, LogRead::append(next));
        assert!(!restarted);

        let (oneshot, ..) = read_node_from(&plugin, "build-1-lint", None).await.unwrap();
        assert_eq!([first, second].concat(), oneshot);
    }

    #[tokio::test]
    async fn resume_writes_a_new_container_header_once_and_skips_finished_containers() {
        let dir = tempfile::tempdir().unwrap();
        let state = live(&[LINT_RUNNING], &[("step-lint", LINT_LOG)]);
        let (server, requests) = live_stub(state.clone());
        let plugin = plugin(&server, &dir);
        let (first, read, _) = read_node_from(&plugin, "build-1-lint", None).await.unwrap();

        let lint = format!("{LINT_LOG}2026-10-28T10:03:09Z z\n");
        update(
            &state,
            &[LINT_DONE, REPORT_RUNNING],
            &[
                ("step-lint", lint.as_str()),
                ("step-report", "2026-10-28T10:03:12Z r1\n"),
            ],
        );
        let (second, read, restarted) =
            read_node_from(&plugin, "build-1-lint", read.cursor.as_ref())
                .await
                .unwrap();
        assert_eq!(
            String::from_utf8(second.clone()).unwrap(),
            format!("2026-10-28T10:03:09Z z\n{REPORT_HEADER}2026-10-28T10:03:12Z r1\n")
        );
        assert_eq!(read.mode, LogReadMode::Append);
        assert!(!restarted);

        let report = "2026-10-28T10:03:12Z r1\n2026-10-28T10:03:13Z r2\n";
        update(
            &state,
            &[LINT_DONE, REPORT_RUNNING],
            &[("step-lint", lint.as_str()), ("step-report", report)],
        );
        sent(&requests);
        let (third, read, restarted) =
            read_node_from(&plugin, "build-1-lint", read.cursor.as_ref())
                .await
                .unwrap();
        assert_eq!(
            String::from_utf8(third.clone()).unwrap(),
            "2026-10-28T10:03:13Z r2\n"
        );
        assert_eq!(read.mode, LogReadMode::Append);
        assert!(!restarted);
        let requested = log_requests(&requests);
        assert_eq!(requested.len(), 1, "{requested:?}");
        assert!(requested[0].contains("container=step-report"));

        let (oneshot, ..) = read_node_from(&plugin, "build-1-lint", None).await.unwrap();
        assert_eq!([first, second, third].concat(), oneshot);
    }

    #[tokio::test]
    async fn a_changed_pod_container_instance_or_rotated_log_restarts_with_the_whole_log() {
        let cases: [(&str, Change); 3] = [
            ("pod", |state| {
                state.lock().unwrap().uid = "uid-2".to_string();
            }),
            ("container instance", |state| {
                state.lock().unwrap().containers[0].1 = "id-restarted".to_string();
            }),
            ("rotated log", |state| {
                state.lock().unwrap().containers[0].2 = "2026-10-28T10:03:20Z only\n".to_string();
            }),
        ];
        for (name, change) in cases {
            let dir = tempfile::tempdir().unwrap();
            let state = live(&[LINT_RUNNING], &[("step-lint", LINT_LOG)]);
            let (server, _requests) = live_stub(state.clone());
            let plugin = plugin(&server, &dir);
            let (_, read, _) = read_node_from(&plugin, "build-1-lint", None).await.unwrap();

            change(&state);
            let (resumed, read, restarted) =
                read_node_from(&plugin, "build-1-lint", read.cursor.as_ref())
                    .await
                    .unwrap();
            let (oneshot, ..) = read_node_from(&plugin, "build-1-lint", None).await.unwrap();

            assert!(restarted, "{name}");
            assert_eq!(read.mode, LogReadMode::Replace, "{name}");
            assert!(read.cursor.is_some(), "{name}");
            assert!(resumed.starts_with(LINT_HEADER.as_bytes()), "{name}");
            assert_eq!(resumed, oneshot, "{name}");
        }

        let dir = tempfile::tempdir().unwrap();
        let state = live(&[LINT_RUNNING], &[("step-lint", LINT_LOG)]);
        let (server, _requests) = live_stub(state);
        let plugin = plugin(&server, &dir);
        let unknown = LogCursor::new(&"junk").unwrap();
        let (bytes, read, restarted) = read_node_from(&plugin, "build-1-lint", Some(&unknown))
            .await
            .unwrap();
        assert!(restarted);
        assert_eq!(read.mode, LogReadMode::Replace);
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            format!("{LINT_HEADER}{LINT_LOG}")
        );
    }

    #[tokio::test]
    async fn a_running_step_cut_at_the_line_limit_is_a_snapshot_without_a_cursor() {
        let body: String = (0..client::STEP_LOG_TAIL_LINES)
            .map(|index| format!("2026-10-28T10:03:02Z line {index}\n"))
            .collect();
        for (step, resumable) in [(LINT_RUNNING, false), (LINT_DONE, true)] {
            let dir = tempfile::tempdir().unwrap();
            let state = live(&[step], &[("step-lint", body.as_str())]);
            let (server, _requests) = live_stub(state);
            let plugin = plugin(&server, &dir);

            let (_, read, _) = read_node_from(&plugin, "build-1-lint", None).await.unwrap();

            assert_eq!(read.mode, LogReadMode::Replace);
            assert_eq!(read.cursor.is_some(), resumable, "{step}");
        }
    }
}
