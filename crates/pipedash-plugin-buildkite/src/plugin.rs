use std::collections::HashMap;

use pipedash_plugin_api::support::{
    collect_tolerant,
    input_str,
    require_configured,
    resume_position,
    string_parameter,
    uuid_id,
    CONCURRENCY,
};
use pipedash_plugin_api::*;

use crate::client::catalog;
use crate::log_dialect::BuildkiteLogDialect;
use crate::{
    client,
    config,
    mapper,
    metadata,
};

#[derive(serde::Serialize, serde::Deserialize)]
struct LogPosition {
    job: String,
    offset: u64,
}

plugin_struct!(
    BuildkitePlugin,
    client::BuildkiteClient,
    metadata::create_metadata();
    status_source: Option<StatusSource> = None
);

impl BuildkitePlugin {
    fn org_and_slug<'a>(id: &'a str, kind: &str) -> PluginResult<(&'a str, &'a str)> {
        match id.split("__").collect::<Vec<_>>().as_slice() {
            [org, slug] => Ok((*org, *slug)),
            _ => Err(PluginError::InvalidConfig(format!(
                "Invalid {kind} ID format: {id}"
            ))),
        }
    }
}

fn buildkite_status_source() -> PluginResult<StatusSource> {
    let url = |value: &str| {
        reqwest::Url::parse(value)
            .map_err(|e| PluginError::Internal(format!("Invalid Buildkite status URL: {e}")))
    };

    Ok(StatusSource {
        name: "Buildkite".to_string(),
        page_url: url("https://www.buildkitestatus.com/")?,
        feed_url: url("https://www.buildkitestatus.com/api/v2/summary.json")?,
        format: FeedFormat::Statuspage,
        components: ComponentFilter::Ids(
            [
                "r21z46hqn47c",
                "1mpkbbd1kh0h",
                "3n7fzwsysx86",
                "56ph4s45xl9s",
            ]
            .map(String::from)
            .to_vec(),
        ),
    })
}

plugin_impl!(BuildkitePlugin [
    metadata,
    status_source,
] {
    fn initialize(
        &mut self, _provider_id: i64, config: HashMap<String, String>,
        http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let token = config
            .get("token")
            .ok_or_else(|| PluginError::InvalidConfig("Missing Buildkite API token".to_string()))?
            .to_string();

        let client = require_client(http_client)?;

        self.client = Some(client::BuildkiteClient::new(client, token));
        self.config = config;
        self.status_source = Some(buildkite_status_source()?);

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client()?;

        client.fetch_organizations().await?;

        Ok(true)
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        catalog::fetch_all_available_pipelines(client, params).await
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<pipedash_plugin_api::Organization>> {
        let client = self.client()?;
        let orgs = client.fetch_organizations().await?;

        Ok(orgs
            .into_iter()
            .map(|org| pipedash_plugin_api::Organization {
                id: org.slug.clone(),
                name: org.name,
                description: org.description,
            })
            .collect())
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        catalog::fetch_available_pipelines_filtered(client, org, search, params).await
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let pipelines = require_configured(config::parse_selected_items(&self.config)?, "pipelines")?;

        let client = self.client()?;
        let requests: Vec<_> = pipelines
            .into_iter()
            .map(|(org, slug)| client.fetch_pipeline(org, slug))
            .collect();
        collect_tolerant(requests, CONCURRENCY, |error| {
            tracing::warn!(error = %error, "Failed to fetch Buildkite pipeline")
        })
        .await
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let (org, slug) = Self::org_and_slug(pipeline_id, "pipeline")?;

        let client = self.client()?;
        let builds = client.fetch_builds(org, slug, limit).await?;

        let pipeline_runs = builds
            .into_iter()
            .map(|build| mapper::build_to_pipeline_run(build, pipeline_id))
            .collect();

        Ok(pipeline_runs)
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let (org, slug) = Self::org_and_slug(pipeline_id, "pipeline")?;
        let build = self.client()?.fetch_build(org, slug, run_number).await?;

        Ok(mapper::build_to_pipeline_run(build, pipeline_id))
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let (org, slug) = Self::org_and_slug(run.pipeline_id, "pipeline")?;
        let jobs = self
            .client()?
            .fetch_build_jobs(org, slug, run.run_number)
            .await?;

        Ok(mapper::jobs_to_log_nodes(&jobs))
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let (org, slug) = Self::org_and_slug(run.pipeline_id, "pipeline")?;
        let node_id = uuid_id(node_id, "Buildkite job ID")?;

        let position =
            resume_position::<LogPosition>(from, sink, |position| position.job == node_id)?;
        let (mode, offset) = self
            .client()?
            .stream_job_log(
                org,
                slug,
                run.run_number,
                node_id,
                position.map(|p| p.offset),
                sink,
            )
            .await?;
        let job = node_id.to_string();
        Ok(LogRead {
            mode,
            cursor: Some(LogCursor::new(&LogPosition { job, offset })?),
        })
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &BuildkiteLogDialect
    }

    async fn fetch_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        Ok(vec![string_parameter(
            "branch",
            "Branch",
            "Branch to build",
            Some("main".to_string()),
            true,
        )])
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<TriggerResult> {
        let (org, slug) = Self::org_and_slug(&params.workflow_id, "workflow")?;
        let branch = input_str(&params, "branch").unwrap_or("main").to_string();

        let client = self.client()?;
        let build = client
            .trigger_build(org, slug, branch, params.inputs)
            .await?;

        Ok(TriggerResult {
            run_id: None,
            run_number: Some(build.number),
        })
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let (org, slug) = Self::org_and_slug(pipeline_id, "pipeline")?;

        let client = self.client()?;
        client.cancel_build(org, slug, run_number).await
    }
});

#[cfg(test)]
mod tests;
