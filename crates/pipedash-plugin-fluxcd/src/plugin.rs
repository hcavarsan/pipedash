use std::collections::HashMap;

use chrono::{
    SecondsFormat,
    Utc,
};
use futures::{
    stream,
    StreamExt,
    TryStreamExt,
};
use pipedash_plugin_api::kubernetes::KubeconfigSource;
use pipedash_plugin_api::logs::tagged::{
    TaggedLogDialect,
    TAGGED_MARKERS,
};
use pipedash_plugin_api::pagination::paginate;
use pipedash_plugin_api::support::{
    organizations,
    CONCURRENCY,
};
use pipedash_plugin_api::*;
use tokio::sync::OnceCell;

use self::catalog::{
    flux_not_installed,
    missing_pipeline,
};
use crate::{
    actions,
    client::FluxClient,
    config::{
        self,
        Settings,
        Target,
    },
    history,
    mapper,
    metadata,
    types::Kind,
    workloads::UnhealthyWorkloads,
};

mod catalog;
mod health;
mod logs;

plugin_struct!(
    custom FluxcdPlugin,
    metadata::create_metadata();
    client: OnceCell<FluxClient> = OnceCell::new(),
    settings: Option<Settings> = None,
    status_source: Option<StatusSource> = None,
    unhealthy: std::sync::Mutex<UnhealthyWorkloads> = std::sync::Mutex::default(),
    degraded: std::sync::Mutex<HashMap<Target, Vec<String>>> = std::sync::Mutex::default()
);

impl FluxcdPlugin {
    fn settings(&self) -> PluginResult<&Settings> {
        self.settings
            .as_ref()
            .ok_or_else(|| PluginError::Internal("FluxCD plugin is not initialized".to_string()))
    }

    async fn client(&self) -> PluginResult<&FluxClient> {
        let settings = self.settings()?;

        self.client
            .get_or_try_init(|| {
                FluxClient::connect(settings.kubeconfig.clone(), settings.context.clone())
            })
            .await
    }

    fn target(&self, pipeline_id: &str) -> PluginResult<Target> {
        let target = config::parse_pipeline_id(pipeline_id)?;
        self.settings()?.ensure_allowed(&target)?;

        Ok(target)
    }
}

plugin_impl!(FluxcdPlugin [
    metadata,
    status_source,
    fetch_log_nodes,
    read_log,
    check_permissions,
    check_health,
] {
    fn can_archive_run(
        &self, pipeline_metadata: &HashMap<String, serde_json::Value>,
        run_metadata: &HashMap<String, serde_json::Value>, status: PipelineStatus,
    ) -> bool {
        history::can_archive_run(pipeline_metadata, run_metadata, status)
    }

    fn configured_repositories(&self, _config: &HashMap<String, String>) -> Vec<String> {
        Vec::new()
    }

    fn pipeline_actions(&self, pipeline: &PipelineFacts<'_>) -> PipelineActionState {
        actions::pipeline_actions(&self.metadata().capabilities, pipeline)
    }

    fn run_actions(&self, pipeline: &PipelineFacts<'_>, run: &RunFacts<'_>) -> RunActionState {
        actions::run_actions(&self.metadata().capabilities, pipeline, run)
    }

    fn run_display(&self, _pipeline: &PipelineFacts<'_>, run: &RunFacts<'_>) -> RunDisplay {
        actions::run_display(run)
    }

    fn initialize(
        &mut self, _provider_id: i64, config: HashMap<String, String>,
        _http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let settings = Settings::from_config(&config)?;
        let status_source = custom_status_source(&config)?;
        self.settings = Some(settings);
        self.status_source = status_source;
        self.client = OnceCell::new();

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client().await?;
        let checks: Vec<(Kind, Option<String>)> = self
            .scopes()?
            .into_iter()
            .flat_map(|scope| Kind::ALL.iter().map(move |kind| (*kind, scope.clone())))
            .collect();
        let served: Vec<bool> = stream::iter(checks)
            .map(|(kind, scope)| async move { client.is_served(kind, scope.as_deref()).await })
            .buffer_unordered(CONCURRENCY)
            .try_collect()
            .await?;

        if served.contains(&true) {
            Ok(true)
        } else {
            Err(flux_not_installed())
        }
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        paginate(self.available().await?, params.unwrap_or_default())
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        let resources = self.discover(&self.settings()?.namespaces).await?;
        Ok(organizations(
            resources
                .iter()
                .map(|resource| resource.metadata.namespace.as_str()),
            Some("Namespace"),
        ))
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let mut available = self.available().await?;

        if let Some(namespace) = org.filter(|namespace| !namespace.is_empty()) {
            available
                .retain(|pipeline| pipeline.organization.as_deref() == Some(namespace.as_str()));
        }
        if let Some(term) = search
            .map(|term| term.trim().to_lowercase())
            .filter(|term| !term.is_empty())
        {
            available.retain(|pipeline| {
                pipeline.id.to_lowercase().contains(&term)
                    || pipeline.name.to_lowercase().contains(&term)
                    || pipeline
                        .repository
                        .as_deref()
                        .is_some_and(|repository| repository.to_lowercase().contains(&term))
                    || pipeline
                        .description
                        .as_deref()
                        .is_some_and(|description| description.to_lowercase().contains(&term))
            });
        }

        paginate(available, params.unwrap_or_default())
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let (found, missing) = self.monitored().await?;
        let mut workload_issues = self.workload_issues(&found).await;

        Ok(found
            .iter()
            .map(|resource| {
                let issues = workload_issues
                    .remove(&mapper::target_of(resource))
                    .unwrap_or_default();
                mapper::map_pipeline(resource, &issues)
            })
            .chain(missing.iter().map(missing_pipeline))
            .collect())
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let (_, _, runs) = self.runs(pipeline_id).await?;

        Ok(runs.into_iter().take(limit).collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        Ok(self.run(pipeline_id, run_number).await?.1)
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &TaggedLogDialect {
            tags: TAGGED_MARKERS,
            infer_errors: false,
        }
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<TriggerResult> {
        let target = self.target(&params.workflow_id)?;
        let client = self.client().await?;
        let resource = client.get(&target).await?;
        if resource.spec.suspend == Some(true) {
            return Err(PluginError::ApiError(format!(
                "{target} is suspended (spec.suspend is true). Resume it with `flux resume` before requesting reconciliation; Pipedash never resumes resources"
            )));
        }

        let requested_at = Utc::now().to_rfc3339_opts(SecondsFormat::Nanos, true);
        client.request_reconcile(&target, &requested_at).await?;

        Ok(TriggerResult::default())
    }

    async fn cancel_run(&self, _pipeline_id: &str, _run_number: i64) -> PluginResult<()> {
        Err(PluginError::NotSupported(
            "Flux reconciliations cannot be cancelled. Suspending or resuming is not exposed by Pipedash".to_string(),
        ))
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
            "namespaces" => {
                let client = FluxClient::connect(
                    KubeconfigSource::from_config(config, config::KUBECONFIG_PATH_KEY),
                    config::context(config),
                )
                .await?;

                match client.namespaces().await {
                    Err(
                        PluginError::AuthenticationFailed(reason)
                        | PluginError::PermissionDenied(reason),
                    ) => {
                        tracing::debug!(%reason, "Namespaces cannot be listed; suggestions unavailable");
                        Ok(Vec::new())
                    }
                    other => other,
                }
            }
            _ => Ok(Vec::new()),
        }
    }
});

#[cfg(test)]
mod tests;
