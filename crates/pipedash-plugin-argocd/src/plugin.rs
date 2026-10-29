use std::collections::HashMap;

use async_trait::async_trait;
use futures::{
    stream,
    StreamExt,
};
use pipedash_plugin_api::*;
use tracing::{
    debug,
    info,
    warn,
};

use crate::client::PodLogRequest;
use crate::log_dialect::ArgocdLogDialect;
use crate::{
    client,
    config,
    logs,
    mapper,
    metadata,
    sign_in,
    types,
};

const DEFAULT_PAGE_SIZE: usize = 1000;
const RESOURCE_TREE_CONCURRENCY: usize = 8;

pub struct ArgocdPlugin {
    metadata: PluginMetadata,
    client: Option<client::ArgocdClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
    server_url: Option<String>,
    status_source: Option<StatusSource>,
    organizations_filter: Option<Vec<String>>,
}

impl Default for ArgocdPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl ArgocdPlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            client: None,
            provider_id: None,
            config: HashMap::new(),
            server_url: None,
            status_source: None,
            organizations_filter: None,
        }
    }

    fn client(&self) -> PluginResult<&client::ArgocdClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }

    fn get_server_url(&self) -> PluginResult<&str> {
        self.server_url
            .as_deref()
            .ok_or_else(|| PluginError::Internal("Server URL not set".to_string()))
    }

    async fn read_pod_log(
        &self, app_name: &str, namespace: &str, pod_name: &str, sink: &mut LogSink,
    ) -> PluginResult<()> {
        let client = self.client()?;
        let pod = client.pod(app_name, namespace, pod_name).await?;
        let events = client
            .pod_events(app_name, namespace, pod_name, &pod.metadata.uid)
            .await?;
        let mut out = Vec::new();
        logs::write_pod_summary(&pod, namespace, pod_name, events, &mut out);
        sink.write(out.into()).await?;

        for container in logs::container_logs(&pod) {
            let mut header = Vec::new();
            container.write_header(&mut header);
            sink.write(header.into()).await?;
            let request = PodLogRequest {
                namespace,
                pod: pod_name,
                container: container.name,
                previous: container.previous,
            };
            if client.stream_pod_log(app_name, request, sink).await? == 0 {
                let mut empty = Vec::new();
                logs::write_no_output(&mut empty);
                sink.write(empty.into()).await?;
            }
        }
        Ok(())
    }

    async fn pipeline_with_live_state(
        client: &client::ArgocdClient, app: &types::Application, provider_id: i64, server_url: &str,
    ) -> Pipeline {
        let mut pipeline = mapper::map_application_to_pipeline(app, provider_id, server_url);
        if mapper::needs_resource_tree(app) {
            match client.resource_tree(&app.metadata.name).await {
                Ok(tree) => mapper::degrade_from_tree(&mut pipeline, &tree),
                Err(error) => warn!(
                    app = %app.metadata.name,
                    %error,
                    "Resource tree unavailable, keeping provider-reported state"
                ),
            }
        }
        pipeline
    }
}

#[async_trait]
impl Plugin for ArgocdPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn provider_type(&self) -> &str {
        "argocd"
    }

    fn client_options(&self, config: &HashMap<String, String>) -> ClientOptions {
        ClientOptions {
            origin: config::get_server_url(config)
                .ok()
                .and_then(|url| reqwest::Url::parse(&url).ok()),
            accept_invalid_certs: config::is_insecure(config),
        }
    }

    fn status_source(&self) -> Option<&StatusSource> {
        self.status_source.as_ref()
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        info!(provider_id, "Initializing ArgoCD plugin");
        debug!(config_keys = ?config.keys().collect::<Vec<_>>());

        let server_url = config::get_server_url(&config)?;
        debug!(server_url, "Configured server URL");

        let token = config::get_token(&config)?;
        debug!(token_length = token.len(), "Retrieved authentication token");

        let insecure = config::is_insecure(&config);
        if insecure {
            warn!("Insecure TLS mode enabled - certificate verification disabled");
        }

        let organizations_filter = config::parse_organizations_filter(&config);
        debug!(?organizations_filter, "Organizations filter configured");

        let status_source = custom_status_source(&config)?;

        let http_client = http_client.ok_or_else(|| {
            PluginError::Internal("ArgoCD plugin requires an HTTP client".to_string())
        })?;
        let client = client::ArgocdClient::new(http_client, &server_url, &token);
        debug!("ArgoCD client created successfully");

        self.client = Some(client);
        self.provider_id = Some(provider_id);
        self.server_url = Some(server_url);
        self.status_source = status_source;
        self.organizations_filter = organizations_filter;
        self.config = config;

        info!("ArgoCD plugin initialization complete");
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        debug!("Validating ArgoCD credentials");
        let client = self.client()?;
        match client.list_applications(None).await {
            Ok(apps) => {
                info!(app_count = apps.len(), "Credentials validated successfully");
                Ok(true)
            }
            Err(e) => {
                warn!(error = ?e, "Credential validation failed");
                Err(e)
            }
        }
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        debug!("Fetching available pipelines");
        let client = self.client()?;

        let mut apps = client.list_applications(None).await?;
        debug!(
            total_apps = apps.len(),
            "Retrieved applications from ArgoCD"
        );

        if let Some(ref orgs_filter) = self.organizations_filter {
            debug!(?orgs_filter, "Filtering applications by organizations");
            apps.retain(|app| {
                let git_org = config::extract_git_org(&app.spec.primary_source().repo_url);
                orgs_filter.contains(&git_org)
            });
            debug!(
                filtered_apps = apps.len(),
                "Applications after organization filtering"
            );
        }

        let total_count = apps.len();
        let mut available_pipelines: Vec<AvailablePipeline> =
            apps.iter().map(mapper::map_available_pipeline).collect();

        let (page, page_size) = if let Some(p) = params {
            let page_num = if p.page == 0 { 1 } else { p.page };
            let size = if p.page_size == 0 {
                DEFAULT_PAGE_SIZE
            } else {
                p.page_size
            };

            let offset = (page_num.saturating_sub(1)) * size;

            available_pipelines = available_pipelines
                .into_iter()
                .skip(offset)
                .take(size)
                .collect();

            (page_num, size)
        } else {
            (1, DEFAULT_PAGE_SIZE)
        };

        debug!(
            pipeline_count = available_pipelines.len(),
            total = total_count,
            page,
            page_size,
            "Mapped and paginated available pipelines"
        );

        Ok(PaginatedResponse::new(
            available_pipelines,
            page,
            page_size,
            total_count,
        ))
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        debug!("Fetching Git organizations from applications");
        let client = self.client()?;

        let apps = client.list_applications(None).await?;

        debug!(
            app_count = apps.len(),
            "Extracting organizations from applications"
        );

        let mut orgs_map: std::collections::HashMap<String, Organization> =
            std::collections::HashMap::new();

        for app in apps {
            let git_org = config::extract_git_org(&app.spec.primary_source().repo_url);

            if git_org == "unknown" || git_org.is_empty() {
                continue;
            }

            if !orgs_map.contains_key(&git_org) {
                orgs_map.insert(
                    git_org.clone(),
                    Organization {
                        id: git_org.clone(),
                        name: git_org.clone(),
                        description: Some(format!("Git Organization: {}", git_org)),
                    },
                );
            }
        }

        let organizations: Vec<Organization> = orgs_map.into_values().collect();
        debug!(
            org_count = organizations.len(),
            org_names = ?organizations.iter().map(|o| &o.name).collect::<Vec<_>>(),
            "Extracted unique Git organizations"
        );

        Ok(organizations)
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        debug!(?org, ?search, "Fetching filtered available pipelines");

        let client = self.client()?;

        let apps = client.list_applications(None).await?;

        debug!(
            total_apps = apps.len(),
            "Retrieved applications for filtering"
        );

        let filtered_apps: Vec<_> = apps
            .into_iter()
            .filter(|app| {
                let org_match = org.as_ref().is_none_or(|o| {
                    let git_org = config::extract_git_org(&app.spec.primary_source().repo_url);
                    git_org == *o
                });

                let search_match = search.as_ref().is_none_or(|s| {
                    app.metadata.name.to_lowercase().contains(&s.to_lowercase())
                        || app
                            .spec
                            .primary_source()
                            .repo_url
                            .to_lowercase()
                            .contains(&s.to_lowercase())
                });

                org_match && search_match
            })
            .collect();

        debug!(
            filtered_apps = filtered_apps.len(),
            "Applications after applying filters"
        );

        let total_count = filtered_apps.len();
        let mut available_pipelines: Vec<AvailablePipeline> = filtered_apps
            .iter()
            .map(mapper::map_available_pipeline)
            .collect();

        let (page, page_size) = if let Some(p) = params {
            let page_num = if p.page == 0 { 1 } else { p.page };
            let size = if p.page_size == 0 {
                DEFAULT_PAGE_SIZE
            } else {
                p.page_size
            };

            let offset = (page_num.saturating_sub(1)) * size;

            available_pipelines = available_pipelines
                .into_iter()
                .skip(offset)
                .take(size)
                .collect();

            (page_num, size)
        } else {
            (1, DEFAULT_PAGE_SIZE)
        };

        debug!(
            pipeline_count = available_pipelines.len(),
            total = total_count,
            page,
            page_size,
            "Mapped and paginated filtered pipelines"
        );

        Ok(PaginatedResponse::new(
            available_pipelines,
            page,
            page_size,
            total_count,
        ))
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        debug!("Fetching configured pipelines");
        let provider_id = self
            .provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))?;

        let client = self.client()?;
        let server_url = self.get_server_url()?;
        debug!(server_url, "Using configured server URL");

        let mut apps = client.list_applications(None).await?;
        debug!(total_apps = apps.len(), "Retrieved all applications");

        if let Some(ref orgs_filter) = self.organizations_filter {
            debug!(?orgs_filter, "Applying organization filter");
            apps.retain(|app| {
                let git_org = config::extract_git_org(&app.spec.primary_source().repo_url);
                orgs_filter.contains(&git_org)
            });
            debug!(
                filtered_apps = apps.len(),
                "Applications after organization filter"
            );
        }

        let filtered_apps = if let Some(selected_items) = config::parse_selected_items(&self.config)
        {
            debug!(
                selected_count = selected_items.len(),
                "Applying user selection filter"
            );
            apps.into_iter()
                .filter(|app| selected_items.contains(&app.metadata.name))
                .collect()
        } else {
            debug!("No user selection - returning all applications");
            apps
        };

        debug!(
            final_apps = filtered_apps.len(),
            "Applications after all filters"
        );

        let live_states: Vec<_> = filtered_apps
            .iter()
            .map(|app| Self::pipeline_with_live_state(client, app, provider_id, server_url))
            .collect();
        let pipelines: Vec<Pipeline> = stream::iter(live_states)
            .buffered(RESOURCE_TREE_CONCURRENCY)
            .collect()
            .await;

        debug!(
            pipeline_count = pipelines.len(),
            "Mapped applications to pipelines"
        );
        Ok(pipelines)
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let (provider_id, _namespace, app_name) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        let server_url = self.get_server_url()?;

        let app = client.get_application(&app_name).await?;

        Ok(mapper::map_runs(&app, provider_id, server_url, limit))
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let (provider_id, _namespace, app_name) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        let server_url = self.get_server_url()?;

        let app = client.get_application(&app_name).await?;

        if let Some(operation) = mapper::map_operation_to_run(&app, provider_id, server_url) {
            if operation.run_number == run_number {
                return Ok(operation);
            }
        }

        let history = app.status.history.as_deref().unwrap_or(&[]);
        let history_item = history
            .iter()
            .find(|h| h.deployed_at.timestamp() == run_number)
            .ok_or_else(|| {
                PluginError::PipelineNotFound(format!("Run {} not found", run_number))
            })?;

        Ok(mapper::map_history_to_run(
            history_item,
            &app,
            provider_id,
            server_url,
        ))
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let (_provider_id, _namespace, app_name) = config::parse_pipeline_id(run.pipeline_id)?;
        let client = self.client()?;
        let (app, tree) = tokio::try_join!(
            client.get_application(&app_name),
            client.resource_tree(&app_name)
        )?;
        let sync = logs::find_sync(&app, run.run_number).ok_or_else(|| {
            PluginError::PipelineNotFound(format!("Run {} not found", run.run_number))
        })?;

        Ok(logs::log_nodes(
            &sync,
            &tree,
            &mapper::app_url(self.get_server_url()?, &app_name),
        ))
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, _from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let (_provider_id, _namespace, app_name) = config::parse_pipeline_id(run.pipeline_id)?;
        let client = self.client()?;
        let mut out = Vec::new();

        if node_id == logs::SYNC_NODE {
            let app = client.get_application(&app_name).await?;
            let sync = logs::find_sync(&app, run.run_number).ok_or_else(|| {
                PluginError::PipelineNotFound(format!("Run {} not found", run.run_number))
            })?;
            logs::write_sync(&sync, &mut out);
        } else if node_id == logs::EVENTS_NODE {
            logs::write_events(client.application_events(&app_name).await?, &mut out);
        } else {
            let (namespace, pod_name) = logs::pod_of_node(node_id).ok_or_else(|| {
                PluginError::InvalidConfig(format!("Invalid ArgoCD log node: {node_id}"))
            })?;
            return self
                .read_pod_log(&app_name, namespace, pod_name, sink)
                .await
                .map(|()| LogRead::replace(None));
        }
        sink.write(out.into())
            .await
            .map(|()| LogRead::replace(None))
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &ArgocdLogDialect
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let (_provider_id, _namespace, app_name) = config::parse_pipeline_id(&params.workflow_id)?;
        let client = self.client()?;

        let inputs = params.inputs.as_ref();

        let revision = inputs
            .and_then(|i| i.get("revision"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let prune = inputs
            .and_then(|i| i.get("prune"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let dry_run = inputs
            .and_then(|i| i.get("dry_run"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let force = inputs
            .and_then(|i| i.get("force"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let apply_only = inputs
            .and_then(|i| i.get("apply_only"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        info!(
            app_name,
            ?revision,
            prune,
            dry_run,
            force,
            apply_only,
            "Triggering ArgoCD sync operation"
        );

        client
            .sync_application(&app_name, revision, prune, dry_run, force, apply_only)
            .await?;

        let sync_type = if dry_run { "Dry run" } else { "Sync" };
        Ok(format!(
            "{} triggered for application: {}",
            sync_type, app_name
        ))
    }

    async fn cancel_run(&self, pipeline_id: &str, _run_number: i64) -> PluginResult<()> {
        let (_provider_id, _namespace, app_name) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;

        client.terminate_operation(&app_name).await?;

        Ok(())
    }

    async fn fetch_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        Ok(vec![
            WorkflowParameter {
                name: "revision".to_string(),
                label: Some("Revision".to_string()),
                description: Some("Git revision (branch, tag, or commit SHA) to sync to. Leave empty to use target revision.".to_string()),
                param_type: WorkflowParameterType::String { default: None },
                required: false,
            },
            WorkflowParameter {
                name: "prune".to_string(),
                label: Some("Prune Resources".to_string()),
                description: Some("Delete resources that are no longer defined in Git".to_string()),
                param_type: WorkflowParameterType::Boolean { default: false },
                required: false,
            },
            WorkflowParameter {
                name: "dry_run".to_string(),
                label: Some("Dry Run".to_string()),
                description: Some("Preview sync without applying changes".to_string()),
                param_type: WorkflowParameterType::Boolean { default: false },
                required: false,
            },
            WorkflowParameter {
                name: "force".to_string(),
                label: Some("Force Sync".to_string(),),
                description: Some("Force sync even if resources are already synced (overrides any state)".to_string()),
                param_type: WorkflowParameterType::Boolean { default: false },
                required: false,
            },
            WorkflowParameter {
                name: "apply_only".to_string(),
                label: Some("Apply Only (Skip Hooks)".to_string()),
                description: Some("Skip pre and post sync hooks".to_string()),
                param_type: WorkflowParameterType::Boolean { default: false },
                required: false,
            },
        ])
    }

    async fn fetch_agents(&self) -> PluginResult<Vec<BuildAgent>> {
        Ok(vec![])
    }

    async fn fetch_artifacts(&self, _run_id: &str) -> PluginResult<Vec<BuildArtifact>> {
        Ok(vec![])
    }

    async fn fetch_queues(&self) -> PluginResult<Vec<BuildQueue>> {
        Ok(vec![])
    }

    fn get_migrations(&self) -> Vec<String> {
        vec![]
    }

    async fn get_field_options(
        &self, field_key: &str, _config: &HashMap<String, String>,
    ) -> PluginResult<Vec<String>> {
        if field_key == "organizations" {
            let apps = self.client()?.list_applications(None).await?;

            let mut git_orgs = std::collections::HashSet::new();
            for app in apps {
                let git_org = config::extract_git_org(&app.spec.primary_source().repo_url);
                if git_org != "unknown" && !git_org.is_empty() {
                    git_orgs.insert(git_org);
                }
            }

            let mut orgs: Vec<String> = git_orgs.into_iter().collect();
            orgs.sort();

            debug!(
                org_count = orgs.len(),
                "Returning Git organizations for dropdown"
            );
            Ok(orgs)
        } else {
            Ok(vec![])
        }
    }

    async fn sign_in_client(
        &self, config: &HashMap<String, String>, http_client: &reqwest::Client,
    ) -> PluginResult<SignInClient> {
        let server_url = config::get_server_url(config)?;
        let settings = client::ArgocdClient::fetch_settings(http_client, &server_url).await?;
        sign_in::client_for(&settings, &server_url)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn cfg(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn client_options_parse_origin_and_insecure() {
        let plugin = ArgocdPlugin::new();
        let options = plugin.client_options(&cfg(&[
            ("server_url", " https://Argo.example.com:8443/ "),
            ("insecure", "true"),
        ]));
        assert_eq!(
            options.origin.as_ref().map(|u| u.as_str()),
            Some("https://argo.example.com:8443/")
        );
        assert!(options.accept_invalid_certs);
    }

    #[test]
    fn client_options_default_secure_for_missing_or_bad_values() {
        let plugin = ArgocdPlugin::new();
        for insecure in ["false", "yes", ""] {
            let options = plugin.client_options(&cfg(&[
                ("server_url", "https://argo.example.com"),
                ("insecure", insecure),
            ]));
            assert!(!options.accept_invalid_certs);
        }
        assert!(
            !plugin
                .client_options(&cfg(&[("server_url", "https://a.io")]))
                .accept_invalid_certs
        );
    }

    #[test]
    fn client_options_origin_none_when_missing_or_invalid() {
        let plugin = ArgocdPlugin::new();
        assert_eq!(plugin.client_options(&cfg(&[])).origin, None);
        assert_eq!(
            plugin
                .client_options(&cfg(&[("server_url", "not a url")]))
                .origin,
            None
        );
    }

    #[test]
    fn initialize_requires_client_and_validates_status_page() {
        let base = [("server_url", "https://a.io"), ("token", "t")];
        let mut plugin = ArgocdPlugin::new();

        assert!(matches!(
            plugin.initialize(1, cfg(&base), None),
            Err(PluginError::Internal(_))
        ));

        let mut bad = base.to_vec();
        bad.push(("status_page_url", "http://example.com"));
        assert!(matches!(
            plugin.initialize(1, cfg(&bad), None),
            Err(PluginError::InvalidConfig(_))
        ));
        assert!(plugin.status_source().is_none());
    }

    const APP: &str = r#"{"metadata":{"name":"web","namespace":"argocd"},
        "spec":{"source":{"repoURL":"https://github.com/acme/web","path":"deploy","targetRevision":"main"},
            "destination":{"server":"https://kubernetes.default.svc","namespace":"web"},"project":"default"},
        "status":{"sync":{"status":"OutOfSync","revision":"def456"},"health":{"status":"Degraded"},
            "history":[{"revision":"abc123","deployedAt":"2026-10-26T09:00:30Z","deployStartedAt":"2026-10-26T09:00:00Z","id":3,
                "source":{"repoURL":"https://ci:s3cret@github.com/acme/web?token=abc","path":"deploy"},"initiatedBy":{"username":"admin"}}],
            "operationState":{"operation":{"sync":{"revision":"def456","prune":true},"initiatedBy":{"automated":true}},
                "phase":"Failed","message":"one or more objects failed to apply","startedAt":"2026-10-27T10:00:00Z",
                "finishedAt":"2026-10-27T10:00:42Z","retryCount":2,
                "syncResult":{"revision":"def456","resources":[
                    {"group":"batch","version":"v1","kind":"Job","namespace":"web","name":"db-migrate","hookType":"PreSync","hookPhase":"Succeeded","syncPhase":"PreSync","message":"job completed"},
                    {"group":"apps","version":"v1","kind":"Deployment","namespace":"web","name":"web","status":"Synced","syncPhase":"Sync","message":"deployment.apps/web configured"},
                    {"group":"","version":"v1","kind":"ConfigMap","namespace":"web","name":"web-config","status":"SyncFailed","syncPhase":"Sync","message":"ConfigMap \"web-config\" is invalid"}]}}}}"#;

    const TREE: &str = r#"{"nodes":[
        {"group":"apps","version":"v1","kind":"Deployment","namespace":"web","name":"web","uid":"d1","health":{"status":"Degraded"}},
        {"version":"v1","kind":"Pod","namespace":"web","name":"web-7f9c-def","uid":"p2","health":{"status":"Healthy"}},
        {"version":"v1","kind":"Pod","namespace":"web","name":"web-7f9c-abc","uid":"p1","health":{"status":"Degraded"}}]}"#;

    const CRASH_TREE: &str = r#"{"nodes":[
        {"group":"apps","version":"v1","kind":"Deployment","namespace":"web","name":"crash","uid":"d1","health":{"status":"Progressing"}},
        {"version":"v1","kind":"Pod","namespace":"web","name":"web-7f9c-def","uid":"p2","health":{"status":"Healthy"}},
        {"version":"v1","kind":"Pod","namespace":"web","name":"web-7f9c-abc","uid":"p1","health":{"status":"Degraded","message":"back-off 40s restarting failed container=app"}},
        {"version":"v1","kind":"Pod","namespace":"web","name":"web-7f9c-ghi","uid":"p3","health":{"status":"Degraded","message":"Back-off pulling image"}}]}"#;

    const ROLLOUT_TREE: &str = r#"{"nodes":[
        {"group":"apps","version":"v1","kind":"Deployment","namespace":"web","name":"rollout","uid":"d1","health":{"status":"Progressing"}},
        {"version":"v1","kind":"Pod","namespace":"web","name":"rollout-1","uid":"p1","health":{"status":"Progressing"}}]}"#;

    fn listed_app(name: &str, health: &str) -> serde_json::Value {
        serde_json::json!({
            "metadata": {"name": name, "namespace": "argocd"},
            "spec": {"source": {"repoURL": "https://github.com/acme/web"}, "destination": {"namespace": "web"}, "project": "default"},
            "status": {"sync": {"status": "Synced"}, "health": {"status": health, "message": format!("{name} is {health}")}},
        })
    }

    const APP_EVENTS: &str = r#"{"metadata":{},"items":[
        {"type":"Warning","reason":"OperationCompleted","message":"Sync operation to def456 failed","count":1,"firstTimestamp":"2026-10-27T10:00:42Z","lastTimestamp":"2026-10-27T10:00:42Z"},
        {"type":"Normal","reason":"OperationStarted","message":"Initiated automated sync to 'def456'","firstTimestamp":"2026-10-27T10:00:00Z","lastTimestamp":"2026-10-27T10:00:00Z","eventTime":null}]}"#;

    const POD_EVENTS: &str = r#"{"metadata":{},"items":[
        {"type":"Warning","reason":"BackOff","message":"Back-off restarting failed container app","count":12,"firstTimestamp":"2026-10-27T10:01:00Z","lastTimestamp":"2026-10-27T10:06:00Z"}]}"#;

    const CRASHING_POD: &str = r#"{"metadata":{"name":"web-7f9c-abc","namespace":"web","uid":"p1"},
        "spec":{"nodeName":"node-1","initContainers":[{"name":"migrate","image":"migrate:1"}],"containers":[{"name":"app","image":"web:2"}]},
        "status":{"phase":"Running",
            "initContainerStatuses":[{"name":"migrate","restartCount":0,"state":{"terminated":{"exitCode":0,"reason":"Completed"}}}],
            "containerStatuses":[{"name":"app","restartCount":3,
                "state":{"waiting":{"reason":"CrashLoopBackOff","message":"back-off 40s restarting failed container=app"}},
                "lastState":{"terminated":{"exitCode":1,"reason":"Error","finishedAt":"2026-10-27T10:05:02Z"}}}]}}"#;

    const HEALTHY_POD: &str = r#"{"metadata":{"name":"web-7f9c-def","namespace":"web","uid":"p2"},
        "spec":{"containers":[{"name":"app"}]},
        "status":{"phase":"Running","containerStatuses":[{"name":"app","restartCount":0,"state":{"running":{}}}]}}"#;

    const LAST: &str = r#"{"result":{"content":"","timeStampStr":"2026-10-27T10:06:00Z","podName":"","last":true}}"#;

    fn log_stream(lines: &[&str]) -> String {
        lines
            .iter()
            .map(|content| {
                format!(
                    "{}\n",
                    serde_json::json!({"result": {
                        "content": content,
                        "timeStampStr": "2026-10-27T10:05:01.5Z",
                        "podName": "web-7f9c-abc",
                        "last": false,
                    }})
                )
            })
            .chain([format!("{LAST}\n")])
            .collect()
    }

    fn argocd_reply(target: &str) -> (&'static str, String) {
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let has = |pair: &str| query.split('&').any(|part| part == pair);
        let ok = |body: &str| ("200 OK", body.to_string());
        match path {
            "/api/v1/settings" => ok(r#"{"url":"","dexConfig":{"connectors":[{"name":"GitHub","type":"github","id":"github"}]}}"#),
            "/api/v1/applications/web" => ok(APP),
            "/api/v1/applications/web/resource-tree" => ok(TREE),
            "/api/v1/applications" => ok(&serde_json::json!({"items": [
                listed_app("crash", "Progressing"),
                listed_app("rollout", "Progressing"),
                listed_app("ready", "Healthy"),
                listed_app("forbidden", "Progressing"),
                listed_app("broken", "Degraded"),
            ]})
            .to_string()),
            "/api/v1/applications/crash/resource-tree" => ok(CRASH_TREE),
            "/api/v1/applications/rollout/resource-tree" => ok(ROLLOUT_TREE),
            "/api/v1/applications/forbidden/resource-tree" => (
                "403 Forbidden",
                r#"{"error":"permission denied: applications, get, default/forbidden","code":7,"message":"permission denied: applications, get, default/forbidden"}"#
                    .to_string(),
            ),
            "/api/v1/applications/broken/resource-tree" => (
                "500 Internal Server Error",
                r#"{"message":"boom"}"#.to_string(),
            ),
            "/api/v1/applications/web/events" if query.is_empty() => ok(APP_EVENTS),
            "/api/v1/applications/web/events" if has("resourceName=web-7f9c-abc") => {
                ok(POD_EVENTS)
            }
            "/api/v1/applications/web/events" => ok(r#"{"metadata":{},"items":null}"#),
            "/api/v1/applications/web/resource" if has("kind=Pod") && has("version=v1") => {
                let pod = if has("resourceName=web-7f9c-abc") {
                    CRASHING_POD
                } else {
                    HEALTHY_POD
                };
                ok(&serde_json::json!({ "manifest": pod }).to_string())
            }
            "/api/v1/applications/web/logs" if has("podName=web-7f9c-def") => (
                "403 Forbidden",
                r#"{"error":"permission denied: logs, get, default/web","code":7,"message":"permission denied: logs, get, default/web"}"#
                    .to_string(),
            ),
            "/api/v1/applications/web/logs" if has("tailLines=1000") && has("follow=false") => {
                if has("container=migrate") {
                    ok(&log_stream(&["migrations applied"]))
                } else if has("previous=true") {
                    ok(&log_stream(&["panic: missing DATABASE_URL"]))
                } else {
                    ok(&format!("{LAST}\n"))
                }
            }
            _ => ("404 Not Found", r#"{"message":"not found"}"#.to_string()),
        }
    }

    async fn stub_argocd(requests: Arc<tokio::sync::Mutex<Vec<String>>>) -> String {
        use tokio::io::{
            AsyncReadExt,
            AsyncWriteExt,
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let requests = requests.clone();
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut buffer = [0; 4096];
                    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                        match stream.read(&mut buffer).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => head.extend_from_slice(&buffer[..n]),
                        }
                    }
                    let head = String::from_utf8_lossy(&head);
                    let target = head.split_whitespace().nth(1).unwrap_or("/").to_string();
                    let (status, body) = argocd_reply(&target);
                    requests.lock().await.push(target);
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        format!("http://{addr}")
    }

    async fn stub_plugin() -> (ArgocdPlugin, Arc<tokio::sync::Mutex<Vec<String>>>) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let requests = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let server_url = stub_argocd(requests.clone()).await;
        let mut plugin = ArgocdPlugin::new();
        plugin
            .initialize(
                1,
                cfg(&[("server_url", &server_url), ("token", "t")]),
                Some(Arc::new(reqwest::Client::new())),
            )
            .unwrap();
        (plugin, requests)
    }

    #[tokio::test]
    async fn sign_in_client_reads_unauthenticated_settings() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let requests = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let server_url = stub_argocd(requests.clone()).await;

        let client = ArgocdPlugin::new()
            .sign_in_client(
                &cfg(&[("server_url", &server_url)]),
                &reqwest::Client::new(),
            )
            .await
            .unwrap();

        assert_eq!(client.issuer, format!("{server_url}/api/dex"));
        assert_eq!(client.label, "GitHub");
        assert_eq!(*requests.lock().await, ["/api/v1/settings"]);
    }

    async fn fetch_states() -> (HashMap<String, Pipeline>, Vec<String>) {
        let (plugin, requests) = stub_plugin().await;
        let pipelines = plugin.fetch_pipelines().await.unwrap();
        let requests = requests.lock().await.clone();
        (
            pipelines
                .into_iter()
                .map(|pipeline| (pipeline.name.clone(), pipeline))
                .collect(),
            requests,
        )
    }

    fn state_of(pipeline: &Pipeline) -> (PipelineStatus, &str, Option<&str>) {
        let state = pipeline.resource_state.as_ref().unwrap();
        (
            pipeline.status,
            state.label.as_str(),
            state.message.as_deref(),
        )
    }

    #[tokio::test]
    async fn progressing_app_with_crash_looping_pod_is_degraded() {
        let (pipelines, _) = fetch_states().await;

        assert_eq!(
            state_of(&pipelines["crash"]),
            (
                PipelineStatus::Degraded,
                "Degraded",
                Some(
                    "Pod web/web-7f9c-abc: back-off 40s restarting failed container=app (+1 more)"
                )
            )
        );
    }

    #[tokio::test]
    async fn progressing_app_with_healthy_tree_stays_progressing() {
        let (pipelines, _) = fetch_states().await;

        assert_eq!(
            state_of(&pipelines["rollout"]),
            (
                PipelineStatus::Running,
                "Progressing",
                Some("rollout is Progressing")
            )
        );
    }

    #[tokio::test]
    async fn healthy_app_costs_no_resource_tree_request() {
        let (pipelines, requests) = fetch_states().await;

        assert_eq!(
            state_of(&pipelines["ready"]),
            (PipelineStatus::Success, "Synced", Some("ready is Healthy"))
        );
        assert!(requests.iter().all(|target| !target.contains("/ready")));
        for unhealthy in ["crash", "rollout", "forbidden", "broken"] {
            let tree = format!("/api/v1/applications/{unhealthy}/resource-tree");
            assert!(requests.contains(&tree), "{unhealthy}");
        }
    }

    #[tokio::test]
    async fn failing_resource_tree_keeps_the_provider_state() {
        let (pipelines, requests) = fetch_states().await;

        assert_eq!(
            state_of(&pipelines["forbidden"]),
            (
                PipelineStatus::Running,
                "Progressing",
                Some("forbidden is Progressing")
            )
        );
        assert_eq!(
            state_of(&pipelines["broken"]),
            (
                PipelineStatus::Degraded,
                "Degraded",
                Some("broken is Degraded")
            )
        );
        let forbidden = "/api/v1/applications/forbidden/resource-tree";
        assert_eq!(requests.iter().filter(|t| *t == forbidden).count(), 1);
    }

    const PIPELINE_ID: &str = "argocd__1__argocd__web";

    fn run_at(stamp: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(stamp)
            .unwrap()
            .timestamp()
    }

    async fn read(
        plugin: &ArgocdPlugin, run_number: i64, node_id: &str,
    ) -> PluginResult<Vec<(LineMarker, String)>> {
        let metadata = HashMap::new();
        let run = RunRef {
            pipeline_id: PIPELINE_ID,
            run_number,
            run_id: "argocd-sync-web-current",
            metadata: &metadata,
        };
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let mut sink = LogSink::new(tx);
        let result = plugin.read_log(run, node_id, None, &mut sink).await;
        drop(sink);
        let mut bytes = Vec::new();
        while let Some(chunk) = rx.recv().await {
            bytes.extend_from_slice(&chunk);
        }
        result?;
        Ok(bytes
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| {
                let parsed = ArgocdLogDialect.parse(line);
                (
                    parsed.marker,
                    String::from_utf8_lossy(parsed.body).into_owned(),
                )
            })
            .collect())
    }

    fn line(marker: LineMarker, text: &str) -> (LineMarker, String) {
        (marker, text.to_string())
    }

    const OPEN: LineMarker = LineMarker::SectionStart { open: true };
    const CLOSED: LineMarker = LineMarker::SectionStart { open: false };

    #[tokio::test]
    async fn log_nodes_list_sync_result_events_and_pods() {
        let (plugin, _) = stub_plugin().await;
        let metadata = HashMap::new();
        let run = RunRef {
            pipeline_id: PIPELINE_ID,
            run_number: run_at("2026-10-27T10:00:00Z"),
            run_id: "argocd-sync-web-current",
            metadata: &metadata,
        };

        let nodes = plugin.fetch_log_nodes(run).await.unwrap();

        let summary: Vec<_> = nodes
            .iter()
            .map(|node| {
                (
                    node.id.as_str(),
                    node.parent_id.as_deref(),
                    node.status,
                    node.log,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (
                    "sync",
                    None,
                    PipelineStatus::Failed,
                    Some(LogAvailability::Ready)
                ),
                (
                    "events",
                    None,
                    PipelineStatus::Success,
                    Some(LogAvailability::Partial)
                ),
                ("pods", None, PipelineStatus::Failed, None),
                (
                    "pod:web:web-7f9c-abc",
                    Some("pods"),
                    PipelineStatus::Failed,
                    Some(LogAvailability::Partial)
                ),
                (
                    "pod:web:web-7f9c-def",
                    Some("pods"),
                    PipelineStatus::Success,
                    Some(LogAvailability::Partial)
                ),
            ]
        );
        assert_eq!(nodes[0].name, "Sync result");
        assert_eq!(nodes[3].name, "web-7f9c-abc");
    }

    #[tokio::test]
    async fn failed_sync_lists_resource_results_by_phase() {
        let (plugin, _) = stub_plugin().await;

        let lines = read(&plugin, run_at("2026-10-27T10:00:00Z"), "sync")
            .await
            .unwrap();

        assert_eq!(
            lines,
            [
                line(OPEN, "Sync operation"),
                line(LineMarker::Plain, "Started by the automated sync policy"),
                line(LineMarker::Plain, "Revision def456"),
                line(LineMarker::Plain, "Options: prune"),
                line(LineMarker::Plain, "Retried 2 times"),
                line(LineMarker::Error, "one or more objects failed to apply"),
                line(LineMarker::Plain, "Failed after 42s"),
                line(CLOSED, "PreSync phase, 1 resource"),
                line(
                    LineMarker::Plain,
                    "Succeeded    batch/Job web/db-migrate (PreSync hook): job completed"
                ),
                line(OPEN, "Sync phase, 2 resources"),
                line(
                    LineMarker::Plain,
                    "Synced       apps/Deployment web/web: deployment.apps/web configured"
                ),
                line(
                    LineMarker::Error,
                    "SyncFailed   ConfigMap web/web-config: ConfigMap \"web-config\" is invalid"
                ),
            ]
        );
    }

    #[tokio::test]
    async fn older_sync_shows_its_history_record() {
        let (plugin, _) = stub_plugin().await;

        let lines = read(&plugin, run_at("2026-10-26T09:00:30Z"), "sync")
            .await
            .unwrap();

        assert_eq!(
            lines,
            [
                line(OPEN, "Sync"),
                line(LineMarker::Plain, "Started by admin"),
                line(LineMarker::Plain, "Revision abc123"),
                line(
                    LineMarker::Plain,
                    "Source https://github.com/acme/web deploy"
                ),
                line(LineMarker::Plain, "Deployed after 30s"),
                line(
                    LineMarker::Plain,
                    "Argo CD keeps per-resource results only for the most recent operation of an app."
                ),
            ]
        );
    }

    #[tokio::test]
    async fn application_events_come_in_time_order() {
        let (plugin, _) = stub_plugin().await;

        let lines = read(&plugin, run_at("2026-10-27T10:00:00Z"), "events")
            .await
            .unwrap();

        assert_eq!(
            lines,
            [
                line(
                    LineMarker::Plain,
                    "Normal OperationStarted: Initiated automated sync to 'def456'"
                ),
                line(
                    LineMarker::Warning,
                    "Warning OperationCompleted: Sync operation to def456 failed"
                ),
            ]
        );
    }

    #[tokio::test]
    async fn crashing_pod_shows_status_events_and_every_container_attempt() {
        let (plugin, requests) = stub_plugin().await;

        let lines = read(
            &plugin,
            run_at("2026-10-27T10:00:00Z"),
            "pod:web:web-7f9c-abc",
        )
        .await
        .unwrap();

        assert_eq!(
            lines,
            [
                line(OPEN, "Pod web/web-7f9c-abc"),
                line(LineMarker::Plain, "Phase Running on node node-1"),
                line(
                    LineMarker::Plain,
                    "Init container migrate: exited with code 0 (Completed)"
                ),
                line(
                    LineMarker::Error,
                    "Container app: waiting (CrashLoopBackOff), 3 restarts, last exit code 1 (Error): back-off 40s restarting failed container=app"
                ),
                line(OPEN, "Events"),
                line(
                    LineMarker::Warning,
                    "Warning BackOff: Back-off restarting failed container app (x12)"
                ),
                line(CLOSED, "Init container migrate"),
                line(LineMarker::Plain, "migrations applied"),
                line(OPEN, "Container app, previous attempt"),
                line(LineMarker::Plain, "panic: missing DATABASE_URL"),
                line(OPEN, "Container app"),
                line(LineMarker::Plain, "No log output."),
            ]
        );
        let requests = requests.lock().await;
        assert!(
            requests
                .iter()
                .any(|r| r.contains("resourceUID=p1") && r.contains("resourceNamespace=web")),
            "{requests:?}"
        );
    }

    #[tokio::test]
    async fn pod_log_without_the_logs_permission_is_permission_denied() {
        let (plugin, _) = stub_plugin().await;

        let error = read(
            &plugin,
            run_at("2026-10-27T10:00:00Z"),
            "pod:web:web-7f9c-def",
        )
        .await
        .unwrap_err();

        assert!(
            matches!(&error, PluginError::PermissionDenied(message) if message.contains("logs, get")),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn unknown_node_ids_are_rejected_without_a_request() {
        let (plugin, requests) = stub_plugin().await;

        for node_id in [
            "",
            "pods",
            "pod:web",
            "pod:web:a?b=1",
            "pod:..:x",
            "pod/web/a",
        ] {
            let error = read(&plugin, run_at("2026-10-27T10:00:00Z"), node_id)
                .await
                .unwrap_err();
            assert!(
                matches!(error, PluginError::InvalidConfig(_)),
                "{node_id}: {error:?}"
            );
        }
        assert!(requests.lock().await.is_empty());
    }
}
