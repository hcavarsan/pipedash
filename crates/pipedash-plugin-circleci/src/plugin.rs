use std::collections::HashMap;
use std::sync::Arc;

use pipedash_plugin_api::config::{
    credential,
    insecure,
};
use pipedash_plugin_api::http::require_client;
use pipedash_plugin_api::support::{
    organizations,
    CONCURRENCY,
};
use pipedash_plugin_api::*;
use tracing::{
    debug,
    info,
    warn,
};

use crate::client::CircleCIClient;
use crate::{
    config,
    log_dialect,
    metadata,
};

mod actions;
mod catalog;
mod logs;

const STEP_OUTPUT_LIMIT: usize = 32 * 1024 * 1024;
const JOB_OUTPUT_LIMIT: usize = 128 * 1024 * 1024;

plugin_struct!(
    CircleCIPlugin,
    CircleCIClient,
    metadata::create_metadata();
    base_url: String = config::DEFAULT_BASE_URL.to_string(),
    step_output_limit: usize = STEP_OUTPUT_LIMIT,
    job_output_limit: usize = JOB_OUTPUT_LIMIT
);

plugin_impl!(CircleCIPlugin [
    metadata,
    available_from_filtered,
    fetch_available_pipelines_filtered,
    fetch_pipelines,
    fetch_run_history,
    fetch_run_details,
    fetch_log_nodes,
    read_log,
    fetch_workflow_parameters,
    trigger_pipeline,
    cancel_run,
    rerun_run,
    rerun_failed_jobs,
] {
    fn configured_repositories(&self, _config: &HashMap<String, String>) -> Vec<String> {
        Vec::new()
    }

    fn pipeline_actions(&self, pipeline: &PipelineFacts<'_>) -> PipelineActionState {
        PipelineActionState {
            rerun_strategy: RerunStrategy::InPlace,
            ..pipedash_plugin_api::plugin::defaults::pipeline_actions(
                &self.metadata().capabilities,
                pipeline,
            )
        }
    }

    fn client_options(&self, config: &HashMap<String, String>) -> ClientOptions {
        ClientOptions {
            origin: reqwest::Url::parse(&config::get_base_url(config)).ok(),
            accept_invalid_certs: insecure(config),
            same_host_redirects: true,
        }
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        info!(provider_id, "Initializing CircleCI plugin");

        let base_url = config::get_base_url(&config);
        let token = credential(&config, "token")?;
        if insecure(&config) {
            warn!("Insecure TLS mode enabled - certificate verification disabled");
        }
        let http_client = require_client(http_client)?;

        self.client = Some(CircleCIClient::new(http_client, token, &base_url)?);
        self.base_url = base_url;
        self.config = config;
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let valid = self.client()?.validate_credentials().await?;
        debug!("CircleCI credentials validated");
        Ok(valid)
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        let projects = self.client()?.fetch_projects().await?;
        Ok(organizations(
            projects
                .iter()
                .map(|project| project.organization.as_str())
                .filter(|name| !name.is_empty()),
            None,
        ))
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &log_dialect::DIALECT
    }
});

#[cfg(test)]
mod tests;
