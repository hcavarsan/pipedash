use std::collections::HashMap;

use pipedash_plugin_api::pagination::paginate;
use pipedash_plugin_api::support::{
    collect_tolerant,
    numeric_id,
    require_configured,
    retain_matching,
    CONCURRENCY,
};
use pipedash_plugin_api::*;

mod actions;

use crate::log_dialect::JenkinsLogDialect;
use crate::{
    client,
    config,
    mapper,
    metadata,
};

plugin_struct!(
    JenkinsPlugin,
    client::JenkinsClient,
    metadata::create_metadata();
    status_source: Option<StatusSource> = None
);

plugin_impl!(JenkinsPlugin [
    metadata,
    status_source,
    trigger_pipeline,
] {
    fn selection_required(&self, config: &HashMap<String, String>) -> bool {
        !config::follow_all_jobs(config)
    }

    fn configured_repositories(&self, config: &HashMap<String, String>) -> Vec<String> {
        if config::follow_all_jobs(config) {
            return Vec::new();
        }
        selected_items(config)
            .map(|item| {
                if item.contains('/') {
                    item.to_string()
                } else {
                    format!("(root)/{item}")
                }
            })
            .collect()
    }

    fn initialize(
        &mut self, _provider_id: i64, config: HashMap<String, String>,
        http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let status_source = custom_status_source(&config)?;

        let token = config
            .get("token")
            .ok_or_else(|| PluginError::InvalidConfig("Missing Jenkins API token".to_string()))?;

        let username = config
            .get("username")
            .ok_or_else(|| PluginError::InvalidConfig("Missing Jenkins username".to_string()))?;

        let auth_value = format!("{username}:{token}");
        let auth_header = format!(
            "Basic {}",
            base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                auth_value.as_bytes()
            )
        );

        let server_url = config
            .get("server_url")
            .ok_or_else(|| PluginError::InvalidConfig("Missing server_url".to_string()))?
            .trim_end_matches('/')
            .to_string();

        let client = require_client(http_client)?;

        self.client = Some(client::JenkinsClient::new(client, server_url, auth_header));
        self.config = config;
        self.status_source = status_source;

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client()?;

        client.discover_all_jobs().await?;
        Ok(true)
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        let all_jobs = client.discover_all_jobs().await?;
        let all_pipelines = client.discovered_jobs_to_available_pipelines(all_jobs);

        paginate(all_pipelines, params.unwrap_or_default())
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let client = self.client()?;
        let fetch = |job_path| client.fetch_pipeline(job_path);
        let requests: Vec<_> = if config::follow_all_jobs(&self.config) {
            client
                .discover_all_jobs()
                .await?
                .into_iter()
                .map(|job| fetch(job.full_path))
                .collect()
        } else {
            require_configured(config::parse_selected_items(&self.config)?, "jobs")?
                .into_iter()
                .map(fetch)
                .collect()
        };
        collect_tolerant(requests, CONCURRENCY, |error| {
            tracing::warn!(error = %error, "Failed to fetch Jenkins pipeline")
        })
        .await
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let job_path = pipeline_id;
        let client = self.client()?;
        let builds = client.fetch_build_history(job_path, limit).await?;

        let pipeline_runs = builds
            .into_iter()
            .map(|build| {
                let encoded_path = config::encode_job_name(job_path);
                mapper::build_to_pipeline_run(
                    build,
                    pipeline_id,
                    client.server_url(),
                    &encoded_path,
                )
            })
            .collect();

        Ok(pipeline_runs)
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let job_path = pipeline_id;
        let client = self.client()?;
        let build = client.fetch_build_details(job_path, run_number).await?;

        let encoded_path = config::encode_job_name(job_path);
        Ok(mapper::build_to_pipeline_run(
            build,
            pipeline_id,
            client.server_url(),
            &encoded_path,
        ))
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let job_path = run.pipeline_id;
        let build = self
            .client()?
            .fetch_build_details(job_path, run.run_number)
            .await?;

        Ok(vec![mapper::build_to_log_node(&build, job_path)])
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let job_path = run.pipeline_id;
        let build_number = numeric_id::<i64>(node_id, "Jenkins build number")?;

        self.client()?
            .stream_console_log(job_path, build_number, from, sink)
            .await
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &JenkinsLogDialect
    }

    async fn fetch_workflow_parameters(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        let start = std::time::Instant::now();
        tracing::debug!(workflow_id = %workflow_id, "Fetching Jenkins workflow parameters");

        let job_path = workflow_id;
        let client = self.client()?;
        let response = client.fetch_job_parameters(job_path).await?;

        let param_definitions: Vec<_> = response
            .property
            .into_iter()
            .filter(|prop| {
                prop._class
                    .as_ref()
                    .map(|c| {
                        c.contains("ParametersDefinitionProperty")
                            || c.contains("ParametersProperty")
                    })
                    .unwrap_or(true)
            })
            .flat_map(|prop| prop.parameter_definitions)
            .collect();

        let parameters = mapper::parameter_definitions_to_workflow_parameters(param_definitions);

        tracing::debug!(
            count = parameters.len(),
            elapsed = ?start.elapsed(),
            "Processed Jenkins workflow parameters"
        );
        Ok(parameters)
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        Ok(vec![Organization {
            id: "default".to_string(),
            name: "Jenkins Server".to_string(),
            description: Some("All accessible Jenkins jobs".to_string()),
        }])
    }

    async fn fetch_available_pipelines_filtered(
        &self, _org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        let all_jobs = client.discover_all_jobs().await?;
        let mut all_pipelines = client.discovered_jobs_to_available_pipelines(all_jobs);

        if let Some(search_term) = search {
            retain_matching(&mut all_pipelines, &search_term);
        }

        paginate(all_pipelines, params.unwrap_or_default())
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let job_path = pipeline_id;
        let client = self.client()?;
        client.cancel_build(job_path, run_number).await
    }
});

#[cfg(test)]
mod tests;
