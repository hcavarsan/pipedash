use std::collections::HashMap;
use std::sync::OnceLock;

use async_trait::async_trait;
use futures::future::{
    join_all,
    try_join_all,
};
use pipedash_plugin_api::*;

use crate::{
    client,
    config,
    mapper,
    metadata,
    types,
};

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
        Self {
            metadata: metadata::create_metadata(),
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

        let new_client =
            client::TektonClient::from_kubeconfig(kubeconfig_path.as_deref(), context.as_deref())
                .await?;

        Ok(self.client.get_or_init(|| new_client))
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
        self.provider_id = Some(provider_id);
        self.config = config;
        Ok(())
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
            let kubeconfig_path = config::get_kubeconfig_path(config);
            let contexts = self.get_available_contexts(kubeconfig_path.as_deref())?;
            Ok(contexts)
        } else if field_key == "namespaces" {
            let kubeconfig_path = config::get_kubeconfig_path(config);
            let context = config::get_context(config);

            match client::TektonClient::from_kubeconfig(
                kubeconfig_path.as_deref(),
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
    use std::sync::mpsc;

    use super::*;

    const CREATED: &str = "2026-10-20T10:00:00Z";

    fn run_json(name: &str, pipeline: &str, uid: &str) -> String {
        format!(
            r#"{{"apiVersion":"tekton.dev/v1","kind":"PipelineRun","metadata":{{"name":"{name}","namespace":"ci","uid":"{uid}","creationTimestamp":"{CREATED}","labels":{{"tekton.dev/pipeline":"{pipeline}"}}}},"spec":{{"pipelineRef":{{"name":"{pipeline}"}}}},"status":{{"startTime":"{CREATED}","conditions":[{{"type":"Running","status":"Unknown","reason":"Running"}}]}}}}"#
        )
    }

    fn stub_apiserver(
        runs: &'static [(&'static str, &'static str, &'static str)],
    ) -> (String, mpsc::Receiver<String>) {
        let (log, requests) = mpsc::channel();
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
                let mut parts = head.split_whitespace();
                let method = parts.next().unwrap_or_default().to_string();
                let target = parts.next().unwrap_or_default().to_string();
                let _ = log.send(format!("{method} {target}"));

                let body = if method == "GET" && target.contains("/pipelines") {
                    let mut names: Vec<&str> =
                        runs.iter().map(|(_, pipeline, _)| *pipeline).collect();
                    names.dedup();
                    let items: Vec<String> = names
                        .iter()
                        .map(|name| {
                            format!(
                                r#"{{"apiVersion":"tekton.dev/v1","kind":"Pipeline","metadata":{{"name":"{name}","namespace":"ci"}},"spec":{{}}}}"#
                            )
                        })
                        .collect();
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
                    "403 Forbidden"
                } else {
                    "200 OK"
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (format!("http://{addr}"), requests)
    }

    fn plugin(server: &str, dir: &tempfile::TempDir) -> TektonPlugin {
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
        let mut plugin = TektonPlugin::new();
        plugin.initialize(1, config, None).unwrap();
        plugin
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
}
