use std::collections::HashMap;

use pipedash_plugin_api::support::CONCURRENCY;
use pipedash_plugin_api::*;
use tokio::sync::OnceCell;

use self::logs::find_run;
use crate::log_dialect::TektonLogDialect;
use crate::{
    client,
    config,
    mapper,
    metadata,
    types,
};

mod actions;
mod catalog;
mod health;
mod logs;

plugin_struct!(
    custom TektonPlugin,
    metadata::create_metadata();
    client: OnceCell<client::TektonClient> = OnceCell::new(),
    status_source: Option<StatusSource> = None,
    config: HashMap<String, String> = HashMap::new()
);

impl TektonPlugin {
    async fn client(&self) -> PluginResult<&client::TektonClient> {
        self.client
            .get_or_try_init(|| async {
                let source = kubernetes::KubeconfigSource::from_config(
                    &self.config,
                    config::KUBECONFIG_PATH_KEY,
                );
                let context = config::get_context(&self.config);

                client::TektonClient::from_kubeconfig(&source, context.as_deref()).await
            })
            .await
    }

    async fn custom_namespaces(&self, client: &client::TektonClient) -> PluginResult<Vec<String>> {
        let manual_namespaces = config::get_namespaces(&self.config);
        if manual_namespaces.is_empty() {
            return Err(PluginError::InvalidConfig(
                "Namespace mode is set to 'custom' but no namespaces are specified. Please provide at least one namespace in the 'namespaces' field (e.g., 'default,tekton-pipelines').".to_string(),
            ));
        }

        client
            .validate_namespaces_have_pipelines(&manual_namespaces)
            .await
    }
}

async fn accessible_namespaces(client: &client::TektonClient) -> PluginResult<Vec<String>> {
    if client.try_list_namespaces_cluster_wide().await?.is_empty() {
        return Err(PluginError::InvalidConfig(
            "No namespaces found in the cluster. Please verify your cluster connection and permissions.".to_string(),
        ));
    }

    client.list_namespaces_with_pipelines().await
}

plugin_impl!(TektonPlugin [
    metadata,
    status_source,
    check_health,
    check_permissions,
    available_from_filtered,
    fetch_pipelines,
    fetch_log_nodes,
    read_log,
    fetch_workflow_parameters,
    trigger_pipeline,
    cancel_run,
    fetch_available_pipelines_filtered,
    get_field_options,
] {
    fn configured_repositories(&self, config: &HashMap<String, String>) -> Vec<String> {
        selected_items(config)
            .map(|item| item.replace("__", "/"))
            .collect()
    }

    fn initialize(
        &mut self, _provider_id: i64, config: HashMap<String, String>,
        _http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let status_source = custom_status_source(&config)?;
        self.config = config;
        self.status_source = status_source;
        self.client = OnceCell::new();
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client().await?;
        let namespace_mode = config::get_namespace_mode(&self.config);

        let namespaces = match namespace_mode {
            config::NamespaceMode::Custom => self.custom_namespaces(client).await?,
            config::NamespaceMode::All => accessible_namespaces(client).await?,
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

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let (namespace, pipeline_name) = config::parse_pipeline_id(pipeline_id)?;
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
            .map(mapper::runs::map_pipeline_run)
            .collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let (namespace, pipeline_name) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client().await?;

        let runs = client
            .list_pipelineruns(&namespace, Some(&pipeline_name))
            .await?;

        let run = find_run(runs, run_number)?;

        Ok(mapper::runs::map_pipeline_run(&run))
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &TektonLogDialect
    }

    fn log_tree_version(&self) -> u32 {
        1
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        Ok(vec![Organization {
            id: "default".to_string(),
            name: "All Namespaces".to_string(),
            description: Some("All accessible Kubernetes namespaces".to_string()),
        }])
    }
});

#[cfg(test)]
mod tests;
