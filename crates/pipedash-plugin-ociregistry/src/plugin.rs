use std::collections::HashMap;
use std::sync::Arc;

use pipedash_plugin_api::config::{
    required_value,
    value,
};
use pipedash_plugin_api::correlation::oci::OciClient;
use pipedash_plugin_api::*;

use crate::metadata;

plugin_struct!(
    custom OciRegistryPlugin, metadata::create_metadata();
    client: Option<OciClient> = None,
    host: String = String::new(),
    credentials: Option<RegistryAuth> = None
);

impl OciRegistryPlugin {
    fn client(&self) -> PluginResult<&OciClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }
}

const HUB_HOSTS: [&str; 3] = ["docker.io", "index.docker.io", "registry-1.docker.io"];
const HUB_TOKEN_HOST: &str = "auth.docker.io";

fn token_hosts(registry: &str, configured: Option<&str>) -> Vec<String> {
    let hub = HUB_HOSTS.contains(&registry).then_some(HUB_TOKEN_HOST);
    configured
        .into_iter()
        .chain(hub)
        .map(str::to_ascii_lowercase)
        .collect()
}

fn not_supported<T>() -> PluginResult<T> {
    Err(PluginError::NotSupported(
        "Container registries have no pipelines or runs".to_string(),
    ))
}

plugin_impl!(OciRegistryPlugin [metadata] {
    fn initialize(
        &mut self, _provider_id: i64, config: HashMap<String, String>,
        http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let host = required_value(&config, "registry_host", "Missing registry host")?
            .to_ascii_lowercase();
        let secret = required_value(&config, "token", "Missing registry password or token")?
            .to_string();
        let username = value(&config, "username").map(str::to_string);
        self.client = Some(OciClient::new(require_client(http_client)?.as_ref().clone()));
        self.credentials = Some(RegistryAuth {
            bearer: username.is_none(),
            username,
            secret,
            token_hosts: token_hosts(&host, value(&config, "token_host")),
        });
        self.host = host;
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        self.client()?
            .ping(&self.host, self.credentials.as_ref())
            .await
    }

    fn selection_required(&self, _config: &HashMap<String, String>) -> bool {
        false
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        Ok(Vec::new())
    }

    async fn fetch_run_history(
        &self, _pipeline_id: &str, _limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        not_supported()
    }

    async fn fetch_run_details(
        &self, _pipeline_id: &str, _run_number: i64,
    ) -> PluginResult<PipelineRun> {
        not_supported()
    }

    async fn trigger_pipeline(&self, _params: TriggerParams) -> PluginResult<TriggerResult> {
        not_supported()
    }

    async fn registry_auth(&self, host: &str) -> PluginResult<Option<RegistryAuth>> {
        Ok(self
            .credentials
            .clone()
            .filter(|_| host.eq_ignore_ascii_case(&self.host)))
    }
});

#[cfg(test)]
mod tests;
