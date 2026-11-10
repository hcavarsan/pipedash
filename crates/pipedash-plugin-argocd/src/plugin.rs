use std::collections::HashMap;

use pipedash_plugin_api::config::{
    credential,
    insecure,
    selected_items,
};
use pipedash_plugin_api::kubernetes::{
    self,
    KubeconfigSource,
};
use pipedash_plugin_api::logs::tagged::{
    TaggedLogDialect,
    TAGGED_MARKERS,
};
use pipedash_plugin_api::support::{
    client_options,
    join_buffered,
};
use pipedash_plugin_api::*;
use tracing::{
    debug,
    info,
    warn,
};

use crate::backend::Backend;
use crate::cluster::ClusterClient;
use crate::{
    client,
    config,
    mapper,
    metadata,
    sign_in,
    types,
};

mod actions;
mod catalog;
mod logs;

const DEFAULT_PAGE_SIZE: usize = 1000;
const RESOURCE_TREE_CONCURRENCY: usize = 8;

plugin_struct!(
    ArgocdPlugin,
    Backend,
    metadata::create_metadata();
    server_url: Option<String> = None,
    status_source: Option<StatusSource> = None,
    organizations_filter: Option<Vec<String>> = None
);

impl ArgocdPlugin {
    fn get_server_url(&self) -> &str {
        self.server_url.as_deref().unwrap_or_default()
    }

    async fn pipeline_with_live_state(
        backend: &Backend, app: &types::Application, server_url: &str,
    ) -> Pipeline {
        let mut pipeline = mapper::map_application_to_pipeline(app, server_url);
        if mapper::needs_resource_tree(app) {
            match backend.health_tree(app).await {
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

plugin_impl!(ArgocdPlugin [
    metadata,
    status_source,
    fetch_available_pipelines,
    fetch_organizations,
    fetch_available_pipelines_filtered,
    fetch_log_nodes,
    read_log,
    trigger_pipeline,
    fetch_workflow_parameters,
] {
    fn configured_repositories(&self, _config: &HashMap<String, String>) -> Vec<String> {
        Vec::new()
    }

    fn pipeline_actions(&self, pipeline: &PipelineFacts<'_>) -> PipelineActionState {
        let mut state = pipedash_plugin_api::plugin::defaults::pipeline_actions(
            &self.metadata().capabilities,
            pipeline,
        );
        let text = |key: &str| {
            pipeline
                .metadata
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
        };
        let value = [text("destination_cluster"), text("destination_namespace")]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" / ");
        state.target = (!value.is_empty()).then(|| DisplayField {
            label: "Destination".to_string(),
            value,
        });
        state
    }

    fn client_options(&self, config: &HashMap<String, String>) -> ClientOptions {
        client_options(
            config::get_server_url(config).ok().as_deref(),
            insecure(config),
        )
    }

    fn initialize(
        &mut self, _provider_id: i64, config: HashMap<String, String>,
        http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        info!("Initializing ArgoCD plugin");
        debug!(config_keys = ?config.keys().collect::<Vec<_>>());

        let organizations_filter = config::parse_organizations_filter(&config);
        debug!(?organizations_filter, "Organizations filter configured");

        let status_source = custom_status_source(&config)?;

        let (backend, server_url) = if config::uses_kubeconfig(&config) {
            let settings = config::ClusterSettings::from_config(&config)?;
            debug!(namespaces = ?settings.namespaces, "Configured Kubernetes connection");
            (Backend::Cluster(ClusterClient::new(settings)), None)
        } else {
            let server_url = config::get_server_url(&config)?;
            debug!(server_url, "Configured server URL");

            let token = credential(&config, "token")?;
            debug!(token_length = token.len(), "Retrieved authentication token");

            if insecure(&config) {
                warn!("Insecure TLS mode enabled - certificate verification disabled");
            }

            let http_client = require_client(http_client)?;
            let client = client::ArgocdClient::new(http_client, &server_url, &token);
            debug!("ArgoCD client created successfully");
            (Backend::Api(client), Some(server_url))
        };

        self.client = Some(backend);
        self.server_url = server_url;
        self.status_source = status_source;
        self.organizations_filter = organizations_filter;
        self.config = config;

        info!("ArgoCD plugin initialization complete");
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        debug!("Validating ArgoCD credentials");
        let client = self.client()?;
        match client.list_applications().await {
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

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        debug!("Fetching configured pipelines");
        let client = self.client()?;
        let server_url = self.get_server_url();
        debug!(server_url, "Using configured server URL");

        let mut apps = client.list_applications().await?;
        debug!(total_apps = apps.len(), "Retrieved all applications");

        if let Some(orgs_filter) = &self.organizations_filter {
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

        let filtered_apps = if let Some(selected_items) = selected_items(&self.config)
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
            .map(|app| Self::pipeline_with_live_state(client, app, server_url))
            .collect();
        let pipelines: Vec<Pipeline> = join_buffered(live_states, RESOURCE_TREE_CONCURRENCY).await;

        debug!(
            pipeline_count = pipelines.len(),
            "Mapped applications to pipelines"
        );
        Ok(pipelines)
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let (namespace, app_name) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        let server_url = self.get_server_url();

        let app = client.get_application(&namespace, &app_name).await?;

        Ok(mapper::runs::map_runs(&app, server_url, limit))
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let (namespace, app_name) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        let server_url = self.get_server_url();

        let app = client.get_application(&namespace, &app_name).await?;

        if let Some(operation) = mapper::runs::map_operation_to_run(&app, server_url) {
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

        Ok(mapper::runs::map_history_to_run(
            history_item,
            &app,
            server_url,
        ))
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &TaggedLogDialect {
            tags: TAGGED_MARKERS,
            infer_errors: false,
        }
    }

    async fn cancel_run(&self, pipeline_id: &str, _run_number: i64) -> PluginResult<()> {
        let (namespace, app_name) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;

        client.terminate_operation(&namespace, &app_name).await?;

        Ok(())
    }

    async fn sign_in_client(
        &self, config: &HashMap<String, String>, http_client: &reqwest::Client,
    ) -> PluginResult<SignInClient> {
        let server_url = config::get_server_url(config)?;
        let settings = client::ArgocdClient::fetch_settings(http_client, &server_url).await?;
        sign_in::client_for(&settings, &server_url)
    }

    fn sign_in_refresh_rejections(&self) -> &'static [SignInRefreshRejection] {
        sign_in::REFRESH_REJECTIONS
    }

    async fn get_field_options(
        &self, field_key: &str, config: &HashMap<String, String>,
    ) -> PluginResult<Vec<String>> {
        match field_key {
            "context" => {
                let source = KubeconfigSource::from_config(config, config::KUBECONFIG_PATH_KEY);
                let description = source.describe();
                kubernetes::available_contexts(move || source.load(), description).await
            }
            _ => Ok(Vec::new()),
        }
    }
});

#[cfg(test)]
mod tests;
