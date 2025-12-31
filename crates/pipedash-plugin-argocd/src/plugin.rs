use std::collections::HashMap;

use async_trait::async_trait;
use pipedash_plugin_api::*;
use tracing::{
    debug,
    info,
    warn,
};

use crate::{
    client,
    config,
    mapper,
    metadata,
};

const DEFAULT_PAGE_SIZE: usize = 1000;

pub struct ArgocdPlugin {
    metadata: PluginMetadata,
    client: Option<client::ArgocdClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
    server_url: Option<String>,
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
}

#[async_trait]
impl Plugin for ArgocdPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn provider_type(&self) -> &str {
        "argocd"
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

        let client = client::ArgocdClient::new(http_client, server_url.clone(), token, insecure)?;
        debug!("ArgoCD client created successfully");

        self.client = Some(client);
        self.provider_id = Some(provider_id);
        self.server_url = Some(server_url);
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
                let git_org = config::extract_git_org(&app.spec.source.repo_url);
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
            let git_org = config::extract_git_org(&app.spec.source.repo_url);

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
                    let git_org = config::extract_git_org(&app.spec.source.repo_url);
                    git_org == *o
                });

                let search_match = search.as_ref().is_none_or(|s| {
                    app.metadata.name.to_lowercase().contains(&s.to_lowercase())
                        || app
                            .spec
                            .source
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
                let git_org = config::extract_git_org(&app.spec.source.repo_url);
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

        let pipelines: Vec<Pipeline> = filtered_apps
            .iter()
            .map(|app| mapper::map_application_to_pipeline(app, provider_id, server_url))
            .collect();

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

        let history = app.status.history.as_deref().unwrap_or(&[]);

        let mut runs: Vec<PipelineRun> = history
            .iter()
            .rev()
            .take(limit)
            .map(|h| mapper::map_history_to_run(h, &app, provider_id, server_url))
            .collect();

        if let Some(current_run) = mapper::map_operation_to_run(&app, provider_id, server_url) {
            runs.insert(0, current_run);
        }

        Ok(runs)
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
        &self, field_key: &str, config: &HashMap<String, String>,
    ) -> PluginResult<Vec<String>> {
        if field_key == "organizations" {
            let server_url = config::get_server_url(config)?;
            let token = config::get_token(config)?;
            let insecure = config::is_insecure(config);

            let temp_client = client::ArgocdClient::new(None, server_url, token, insecure)?;

            let apps = temp_client.list_applications(None).await?;

            let mut git_orgs = std::collections::HashSet::new();
            for app in apps {
                let git_org = config::extract_git_org(&app.spec.source.repo_url);
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
}
