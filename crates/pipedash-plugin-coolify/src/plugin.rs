use std::collections::HashMap;
use std::sync::Arc;

use pipedash_plugin_api::config::{
    credential,
    insecure,
    selected_items,
};
use pipedash_plugin_api::http::require_client;
use pipedash_plugin_api::support::{
    client_options,
    organizations,
    try_buffered,
    CONCURRENCY,
};
use pipedash_plugin_api::*;
use tracing::{
    debug,
    info,
    warn,
};

use crate::{
    client,
    config,
    log_dialect,
    mapper,
    metadata,
    types,
};

mod catalog;
mod logs;

plugin_struct!(
    CoolifyPlugin,
    client::CoolifyClient,
    metadata::create_metadata();
    server_url: String = String::new()
);

plugin_impl!(CoolifyPlugin [
    metadata,
    available_from_filtered,
    fetch_available_pipelines_filtered,
    fetch_log_nodes,
    read_log,
] {
    fn configured_repositories(&self, _config: &HashMap<String, String>) -> Vec<String> {
        Vec::new()
    }

    fn client_options(&self, config: &HashMap<String, String>) -> ClientOptions {
        client_options(
            config::get_server_url(config).ok().as_deref(),
            insecure(config),
        )
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        info!(provider_id, "Initializing Coolify plugin");

        let server_url = config::get_server_url(&config)?;
        let token = credential(&config, "token")?;
        if insecure(&config) {
            warn!("Insecure TLS mode enabled - certificate verification disabled");
        }
        let http_client = require_client(http_client)?;

        self.client = Some(client::CoolifyClient::new(
            http_client,
            server_url.clone(),
            token,
        ));
        self.server_url = server_url;
        self.config = config;
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let version = self.client()?.version().await?;
        debug!(version, "Coolify credentials validated");
        Ok(true)
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        let projects = self.client()?.projects().await?;
        Ok(organizations(
            projects
                .iter()
                .map(|project| project.name.as_str())
                .filter(|name| !name.is_empty()),
            None,
        ))
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let selected = selected_items(&self.config);
        let client = self.client()?;
        let applications = client.applications().await?;

        let listed: Vec<&types::Application> = applications
            .iter()
            .filter(|app| {
                selected.as_ref().is_none_or(|items| {
                    items
                        .iter()
                        .any(|item| *item == app.uuid || *item == app.name)
                })
            })
            .collect();
        let requests: Vec<_> = listed
            .iter()
            .map(|app| client.deployments(&app.uuid, 0, 1))
            .collect();
        let latest: Vec<Vec<types::Deployment>> = try_buffered(requests, CONCURRENCY).await?;

        let pipelines: Vec<Pipeline> = listed
            .iter()
            .zip(&latest)
            .map(|(app, page)| mapper::map_pipeline(app, page.first()))
            .collect();

        debug!(
            applications = applications.len(),
            pipelines = pipelines.len(),
            "Mapped Coolify applications to pipelines"
        );
        Ok(pipelines)
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let app_uuid = config::parse_pipeline_id(pipeline_id)?;
        let deployments = self.client()?.recent_deployments(app_uuid, limit).await?;

        Ok(deployments
            .iter()
            .map(|deployment| mapper::map_run(pipeline_id, &self.server_url, deployment))
            .collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let app_uuid = config::parse_pipeline_id(pipeline_id)?;
        let deployment = self.client()?.find_deployment(app_uuid, run_number).await?;

        Ok(mapper::map_run(pipeline_id, &self.server_url, &deployment))
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &log_dialect::DIALECT
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<TriggerResult> {
        let app_uuid = config::parse_pipeline_id(&params.workflow_id)?;
        let client = self.client()?;
        let queued = client
            .deploy(app_uuid)
            .await?
            .deployments
            .into_iter()
            .next();
        let message = queued
            .as_ref()
            .and_then(|queued| queued.message.as_deref())
            .filter(|message| !message.is_empty())
            .map(str::to_string);
        let Some(deployment_uuid) = queued
            .and_then(|queued| queued.deployment_uuid)
            .filter(|uuid| !uuid.is_empty())
        else {
            return Err(PluginError::ApiError(format!(
                "Coolify did not queue a deployment of application {app_uuid}: {}",
                message.as_deref().unwrap_or("no reason given")
            )));
        };

        let run_number = match client.deployment(&deployment_uuid).await {
            Ok(detail) => Some(detail.id),
            Err(error) => {
                warn!(
                    app_uuid,
                    deployment_uuid,
                    error = %error,
                    "Coolify deploy queued but its details could not be read"
                );
                None
            }
        };
        info!(app_uuid, deployment_uuid, "Triggered Coolify deploy");

        Ok(TriggerResult {
            run_id: None,
            run_number,
        })
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let app_uuid = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        let deployment = client.find_deployment(app_uuid, run_number).await?;
        client.cancel(&deployment.deployment_uuid).await?;
        info!(app_uuid, run_number, "Cancelled Coolify deployment");
        Ok(())
    }
});

#[cfg(test)]
mod tests;
