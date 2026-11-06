use std::collections::{
    BTreeSet,
    HashMap,
};
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures::{
    stream,
    StreamExt,
    TryStreamExt,
};
use pipedash_plugin_api::*;
use serde_json::{
    json,
    Value,
};
use tracing::{
    debug,
    info,
    warn,
};

use crate::client::CircleCIClient;
use crate::log_cursor::RenderedCursor;
use crate::log_dialect::{
    self,
    Section,
};
use crate::types::{
    CirclePipeline,
    PipelineDefinition,
    Project,
    Workflow,
};
use crate::{
    config,
    mapper,
    metadata,
    parameters,
};

const DEFAULT_PAGE_SIZE: usize = 100;
const CONCURRENCY: usize = 8;
const LOG_CHUNK_BYTES: usize = 64 * 1024;
const FAILED_STEP_STATUSES: [&str; 4] = ["failed", "timedout", "infrastructure_fail", "canceled"];
const STEP_OUTPUT_LIMIT: usize = 32 * 1024 * 1024;
const JOB_OUTPUT_LIMIT: usize = 128 * 1024 * 1024;

pub struct CircleCIPlugin {
    metadata: PluginMetadata,
    client: Option<CircleCIClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
    base_url: String,
    step_output_limit: usize,
    job_output_limit: usize,
}

impl Default for CircleCIPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl CircleCIPlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            client: None,
            provider_id: None,
            config: HashMap::new(),
            base_url: config::DEFAULT_BASE_URL.to_string(),
            step_output_limit: STEP_OUTPUT_LIMIT,
            job_output_limit: JOB_OUTPUT_LIMIT,
        }
    }

    fn client(&self) -> PluginResult<&CircleCIClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }

    fn provider_id(&self) -> PluginResult<i64> {
        self.provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))
    }

    async fn selected_projects(&self) -> PluginResult<Vec<Project>> {
        let client = self.client()?;
        let Some(selected) = config::parse_selected_items(&self.config) else {
            return client.fetch_projects().await;
        };
        let requests: Vec<_> = selected
            .iter()
            .map(|slug| client.fetch_project(slug))
            .collect();
        stream::iter(requests)
            .buffered(CONCURRENCY)
            .try_collect()
            .await
    }

    async fn to_run(
        &self, pipeline_id: &str, slug: &str, pipeline: &CirclePipeline,
    ) -> PluginResult<PipelineRun> {
        let workflows = self.client()?.fetch_workflows(&pipeline.id).await?;
        Ok(mapper::to_run(
            pipeline_id,
            pipeline,
            &workflows,
            &self.base_url,
            slug,
        ))
    }

    async fn runs(
        &self, pipeline_id: &str, slug: &str, pipelines: &[CirclePipeline],
    ) -> PluginResult<Vec<PipelineRun>> {
        let requests: Vec<_> = pipelines
            .iter()
            .map(|pipeline| self.to_run(pipeline_id, slug, pipeline))
            .collect();
        stream::iter(requests)
            .buffered(CONCURRENCY)
            .try_collect()
            .await
    }

    async fn latest_run(&self, project: &Project) -> PluginResult<Option<PipelineRun>> {
        let pipeline_id = config::build_pipeline_id(self.provider_id()?, &project.slug);
        let pipelines = self.client()?.fetch_pipelines(&project.slug, 1).await?;
        match pipelines.first() {
            Some(pipeline) => Ok(Some(
                self.to_run(&pipeline_id, &project.slug, pipeline).await?,
            )),
            None => Ok(None),
        }
    }

    async fn run_workflows(
        &self, slug: &str, run_number: i64,
    ) -> PluginResult<(CirclePipeline, Vec<Workflow>)> {
        let client = self.client()?;
        let pipeline = client.fetch_pipeline(slug, run_number).await?;
        let workflows = client.fetch_workflows(&pipeline.id).await?;
        Ok((pipeline, workflows))
    }

    async fn definitions(&self, project: &Project) -> PluginResult<Vec<PipelineDefinition>> {
        let project_id = project.id.as_deref().ok_or_else(|| {
            PluginError::ApiError(format!(
                "CircleCI did not return an id for project {}",
                project.slug
            ))
        })?;
        self.client()?.fetch_definitions(project_id).await
    }

    async fn pipeline_parameters(&self, slug: &str) -> PluginResult<Vec<WorkflowParameter>> {
        let client = self.client()?;
        let Some(latest) = client.fetch_pipelines(slug, 1).await?.into_iter().next() else {
            return Ok(Vec::new());
        };
        let Some(source) = client.fetch_pipeline_config(&latest.id).await? else {
            return Ok(Vec::new());
        };
        match parameters::pipeline_parameters(&source) {
            Ok(declared) => Ok(declared),
            Err(error) => {
                warn!(slug, error = %error, "CircleCI pipeline parameters could not be read");
                Ok(Vec::new())
            }
        }
    }

    async fn job_sections(&self, slug: &str, job_number: i64) -> PluginResult<Vec<u8>> {
        let client = self.client()?;
        let steps = client.fetch_job_steps(slug, job_number).await?;
        let actions: Vec<_> = steps
            .steps
            .iter()
            .flat_map(|step| {
                let parallel = step.actions.len() > 1;
                step.actions
                    .iter()
                    .map(move |action| (step, action, parallel))
            })
            .collect();
        let requests: Vec<_> = actions
            .iter()
            .map(|(_, action, _)| async move {
                match action.output_url.as_deref().filter(|url| !url.is_empty()) {
                    Some(url) => client.fetch_step_output(url, self.step_output_limit).await,
                    None => Ok(Some(Vec::new())),
                }
            })
            .collect();
        let mut downloads = stream::iter(requests).buffered(CONCURRENCY);
        let mut outputs = Vec::with_capacity(actions.len());
        let mut total = 0usize;
        while let Some(output) = downloads.next().await {
            let output = output?;
            total += output
                .iter()
                .flatten()
                .map(|message| message.message.len())
                .sum::<usize>();
            if total > self.job_output_limit {
                break;
            }
            outputs.push(output);
        }
        drop(downloads);
        outputs.resize_with(actions.len(), || None);
        let sections: Vec<Section<'_>> = actions
            .iter()
            .zip(&outputs)
            .map(|((step, action, parallel), output)| Section {
                name: if *parallel {
                    format!("{} (container {})", step.name, action.index)
                } else {
                    step.name.clone()
                },
                failed: action.failed == Some(true)
                    || action
                        .status
                        .as_deref()
                        .is_some_and(|status| FAILED_STEP_STATUSES.contains(&status)),
                started_at: action.start_time,
                output: output.as_deref(),
            })
            .collect();
        Ok(log_dialect::render(&sections))
    }

    async fn write_chunks(mut bytes: Bytes, sink: &mut LogSink) -> PluginResult<()> {
        while !bytes.is_empty() {
            sink.write(bytes.split_to(bytes.len().min(LOG_CHUNK_BYTES)))
                .await?;
        }
        Ok(())
    }
}

fn job_number(node_id: &str) -> PluginResult<i64> {
    Some(node_id)
        .filter(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|id| id.parse().ok())
        .ok_or_else(|| PluginError::InvalidConfig(format!("Invalid log node ID: {node_id}")))
}

fn pick_definition<'a>(
    definitions: &'a [PipelineDefinition], chosen: Option<&str>,
) -> PluginResult<&'a PipelineDefinition> {
    match chosen {
        Some(chosen) => definitions
            .iter()
            .find(|definition| definition.id == chosen || definition.name == chosen)
            .ok_or_else(|| {
                PluginError::InvalidConfig(format!("CircleCI has no pipeline named {chosen}"))
            }),
        None => match definitions {
            [only] => Ok(only),
            [] => Err(PluginError::InvalidConfig(
                "This CircleCI project has no pipelines to run".to_string(),
            )),
            _ => Err(PluginError::InvalidConfig(
                "Choose which CircleCI pipeline to run".to_string(),
            )),
        },
    }
}

#[async_trait]
impl Plugin for CircleCIPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn provider_type(&self) -> &str {
        "circleci"
    }

    fn client_options(&self, config: &HashMap<String, String>) -> ClientOptions {
        ClientOptions {
            origin: reqwest::Url::parse(&config::get_base_url(config)).ok(),
            accept_invalid_certs: config::is_insecure(config),
            same_host_redirects: true,
        }
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        info!(provider_id, "Initializing CircleCI plugin");

        let base_url = config::get_base_url(&config);
        let token = config::get_credential(&config, "token")?;
        if config::is_insecure(&config) {
            warn!("Insecure TLS mode enabled - certificate verification disabled");
        }
        let http_client = http_client.ok_or_else(|| {
            PluginError::Internal("CircleCI plugin requires an HTTP client".to_string())
        })?;

        self.client = Some(CircleCIClient::new(http_client, token, &base_url)?);
        self.provider_id = Some(provider_id);
        self.base_url = base_url;
        self.config = config;
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let valid = self.client()?.validate_credentials().await?;
        debug!("CircleCI credentials validated");
        Ok(valid)
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        self.fetch_available_pipelines_filtered(None, None, params)
            .await
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        let projects = self.client()?.fetch_projects().await?;
        let names: BTreeSet<&str> = projects
            .iter()
            .map(|project| project.organization.as_str())
            .filter(|name| !name.is_empty())
            .collect();

        Ok(names
            .into_iter()
            .map(|name| Organization {
                id: name.to_string(),
                name: name.to_string(),
                description: None,
            })
            .collect())
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let projects = self.client()?.fetch_projects().await?;
        let search = search.map(|term| term.to_lowercase());
        let matching: Vec<AvailablePipeline> = projects
            .iter()
            .map(mapper::to_available_pipeline)
            .filter(|project| org.is_none() || project.organization == org)
            .filter(|project| {
                search.as_ref().is_none_or(|term| {
                    project.name.to_lowercase().contains(term)
                        || project.id.to_lowercase().contains(term)
                })
            })
            .collect();

        let total_count = matching.len();
        let params = params.unwrap_or_default();
        let page = params.page.max(1);
        let page_size = if params.page_size == 0 {
            DEFAULT_PAGE_SIZE
        } else {
            params.page_size
        };
        let offset = (page - 1).saturating_mul(page_size);
        let items: Vec<AvailablePipeline> =
            matching.into_iter().skip(offset).take(page_size).collect();

        let has_more = offset + items.len() < total_count;
        let mut response = PaginatedResponse::new(items, page, page_size, total_count);
        response.has_more = has_more;
        Ok(response)
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self.provider_id()?;
        let projects = self.selected_projects().await?;
        let requests: Vec<_> = projects
            .iter()
            .map(|project| self.latest_run(project))
            .collect();
        let latest: Vec<Option<PipelineRun>> = stream::iter(requests)
            .buffered(CONCURRENCY)
            .try_collect()
            .await?;

        let pipelines: Vec<Pipeline> = projects
            .iter()
            .zip(&latest)
            .map(|(project, run)| mapper::to_pipeline(provider_id, project, run.as_ref()))
            .collect();
        debug!(
            pipelines = pipelines.len(),
            "Mapped CircleCI projects to pipelines"
        );
        Ok(pipelines)
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let (_, slug) = config::parse_pipeline_id(pipeline_id)?;
        let pipelines = self.client()?.fetch_pipelines(slug, limit).await?;
        self.runs(pipeline_id, slug, &pipelines).await
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let (_, slug) = config::parse_pipeline_id(pipeline_id)?;
        let pipeline = self.client()?.fetch_pipeline(slug, run_number).await?;
        self.to_run(pipeline_id, slug, &pipeline).await
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let (_, slug) = config::parse_pipeline_id(run.pipeline_id)?;
        let client = self.client()?;
        let pipeline_uuid = match run
            .metadata
            .get("circleci_pipeline_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
        {
            Some(id) => id.to_string(),
            None => client.fetch_pipeline(slug, run.run_number).await?.id,
        };
        let workflows = client.fetch_workflows(&pipeline_uuid).await?;
        let latest = mapper::latest_workflows(&workflows);
        let requests: Vec<_> = latest
            .iter()
            .map(|workflow| client.fetch_jobs(&workflow.id))
            .collect();
        let jobs: Vec<_> = stream::iter(requests)
            .buffered(CONCURRENCY)
            .try_collect()
            .await?;
        let grouped: Vec<_> = latest.into_iter().zip(jobs).collect();
        Ok(mapper::log_nodes(
            &self.base_url,
            slug,
            run.run_number,
            &grouped,
        ))
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let job_number = job_number(node_id)?;
        let (_, slug) = config::parse_pipeline_id(run.pipeline_id)?;
        let rendered = Bytes::from(self.job_sections(slug, job_number).await?);

        let resumed = match from {
            None => None,
            Some(cursor) => {
                let resumed = cursor
                    .parse::<RenderedCursor>()
                    .and_then(|cursor| cursor.resume(&rendered));
                if resumed.is_none() {
                    sink.restart()?;
                }
                resumed
            }
        };
        let next = LogCursor::new(&RenderedCursor::of(&rendered))?;
        match resumed {
            Some(offset) => {
                Self::write_chunks(rendered.slice(offset..), sink).await?;
                Ok(LogRead::append(next))
            }
            None => {
                Self::write_chunks(rendered, sink).await?;
                Ok(LogRead::replace(Some(next)))
            }
        }
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &log_dialect::CircleCILogDialect
    }

    async fn fetch_workflow_parameters(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        let (_, slug) = config::parse_pipeline_id(workflow_id)?;
        let project = self.client()?.fetch_project(slug).await?;
        let mut declared = vec![WorkflowParameter {
            name: parameters::BRANCH.to_string(),
            label: Some("Branch".to_string()),
            description: Some("Branch to build".to_string()),
            param_type: WorkflowParameterType::String {
                default: project.default_branch.clone(),
            },
            required: true,
        }];
        if slug.starts_with("circleci/") {
            let definitions = self.definitions(&project).await?;
            if definitions.len() > 1 {
                declared.push(WorkflowParameter {
                    name: parameters::DEFINITION.to_string(),
                    label: Some("Pipeline".to_string()),
                    description: Some("CircleCI pipeline to run".to_string()),
                    param_type: WorkflowParameterType::Choice {
                        options: definitions.iter().map(|d| d.name.clone()).collect(),
                        default: definitions.first().map(|d| d.name.clone()),
                    },
                    required: true,
                });
            }
        }
        declared.extend(self.pipeline_parameters(slug).await?);
        Ok(declared)
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let (_, slug) = config::parse_pipeline_id(&params.workflow_id)?;
        let client = self.client()?;
        let inputs = parameters::trigger_inputs(params.inputs);
        let app_project = slug.starts_with("circleci/");
        let project = if app_project || inputs.branch.is_none() {
            Some(client.fetch_project(slug).await?)
        } else {
            None
        };
        let branch = inputs
            .branch
            .or_else(|| project.as_ref().and_then(|p| p.default_branch.clone()))
            .ok_or_else(|| PluginError::InvalidConfig("Choose a branch to build".to_string()))?;

        let number = match project.as_ref().filter(|_| app_project) {
            Some(project) => {
                let definitions = self.definitions(project).await?;
                let definition = pick_definition(&definitions, inputs.definition.as_deref())?;
                let mut body = json!({
                    "definition_id": definition.id,
                    "config": {"branch": branch},
                    "checkout": {"branch": branch},
                });
                if !inputs.parameters.is_empty() {
                    body["parameters"] = Value::Object(inputs.parameters);
                }
                client.run_pipeline(slug, &body).await?
            }
            None => {
                let mut body = json!({ "branch": branch });
                if !inputs.parameters.is_empty() {
                    body["parameters"] = Value::Object(inputs.parameters);
                }
                client.trigger(slug, &body).await?
            }
        };
        info!(slug, number, "Triggered CircleCI pipeline");

        Ok(json!({
            "message": format!("Triggered pipeline #{number} on {branch}"),
            "run_number": number,
            "logs_url": mapper::pipeline_url(&self.base_url, slug, number),
        })
        .to_string())
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let (_, slug) = config::parse_pipeline_id(pipeline_id)?;
        let (_, workflows) = self.run_workflows(slug, run_number).await?;
        let active: Vec<&Workflow> = mapper::latest_workflows(&workflows)
            .into_iter()
            .filter(|workflow| mapper::is_active(&workflow.status))
            .collect();
        if active.is_empty() {
            return Err(PluginError::ApiError(format!(
                "Pipeline #{run_number} has no running workflows to cancel"
            )));
        }
        let client = self.client()?;
        for workflow in active {
            client.cancel(&workflow.id).await?;
        }
        info!(slug, run_number, "Cancelled CircleCI pipeline");
        Ok(())
    }

    async fn rerun_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        self.rerun(pipeline_id, run_number, false).await
    }

    async fn rerun_failed_jobs(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        self.rerun(pipeline_id, run_number, true).await
    }
}

impl CircleCIPlugin {
    async fn rerun(
        &self, pipeline_id: &str, run_number: i64, failed_only: bool,
    ) -> PluginResult<()> {
        let (_, slug) = config::parse_pipeline_id(pipeline_id)?;
        let (_, workflows) = self.run_workflows(slug, run_number).await?;
        let latest = mapper::latest_workflows(&workflows);
        if latest
            .iter()
            .any(|workflow| mapper::is_active(&workflow.status))
        {
            return Err(PluginError::ApiError(format!(
                "Pipeline #{run_number} is still running. Wait for it to finish or cancel it"
            )));
        }
        let chosen: Vec<&Workflow> = latest
            .into_iter()
            .filter(|workflow| {
                !failed_only || matches!(workflow.status.as_str(), "failed" | "error")
            })
            .collect();
        if chosen.is_empty() {
            return Err(PluginError::ApiError(format!(
                "Pipeline #{run_number} has no {}workflows to rerun",
                if failed_only { "failed " } else { "" }
            )));
        }
        let client = self.client()?;
        for workflow in chosen {
            client.rerun(&workflow.id, failed_only).await?;
        }
        info!(slug, run_number, failed_only, "Reran CircleCI pipeline");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };

    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };
    use tokio::sync::Mutex;

    use super::*;

    const TOKEN: &str = "T-stub";
    const WEB: &str = "circleci__1__gh/acme/web";
    const APP: &str = "circleci__1__circleci/org1/proj1";

    struct Recorded {
        method: String,
        path: String,
        query: String,
        headers: HashMap<String, String>,
        body: Value,
    }

    type Requests = Arc<Mutex<Vec<Recorded>>>;

    struct Stub {
        plugin: CircleCIPlugin,
        requests: Requests,
        growth: Arc<AtomicUsize>,
    }

    fn cfg(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn project(slug: &str, id: &str, name: &str, branch: &str) -> Value {
        json!({"slug": slug, "id": id, "name": name, "organization_name": "acme",
            "vcs_info": {"vcs_url": format!("https://github.com/acme/{name}"), "default_branch": branch}})
    }

    fn pipeline(number: i64) -> Value {
        json!({"id": format!("pipe-{number}"), "number": number, "state": "created",
            "created_at": "2026-11-01T10:00:00Z", "updated_at": "2026-11-01T10:10:00Z",
            "vcs": {"branch": "main", "revision": "abc123", "commit": {"subject": "Ship it"}},
            "trigger": {"actor": {"login": "ada"}}})
    }

    fn workflow(id: &str, name: &str, status: &str, minute: u32) -> Value {
        json!({"id": id, "name": name, "status": status,
            "created_at": format!("2026-11-01T10:{minute:02}:00Z"),
            "stopped_at": format!("2026-11-01T10:{:02}:00Z", minute + 3)})
    }

    fn page(items: Value) -> Value {
        json!({"items": items, "next_page_token": null})
    }

    fn workflows(pipeline_id: &str) -> Option<Value> {
        Some(match pipeline_id {
            "pipe-42" => json!([
                workflow("wf-build-1", "build", "failed", 0),
                workflow("wf-build-2", "build", "running", 5),
                workflow("wf-deploy", "deploy", "on_hold", 1),
            ]),
            "pipe-41" => json!([
                workflow("wf-41-build", "build", "failed", 0),
                workflow("wf-41-lint", "lint", "success", 0),
            ]),
            _ => return None,
        })
    }

    fn action(index: i64, status: &str, output: Option<String>) -> Value {
        json!({"index": index, "status": status, "failed": status == "failed",
            "output_url": output, "start_time": "2026-11-01T10:05:00Z"})
    }

    fn steps(server: &str, growth: usize) -> Value {
        let out = |name: &str| Some(format!("{server}/out/{name}?X-Amz-Signature=sig"));
        let second = if growth > 0 {
            action(1, "failed", out("t1"))
        } else {
            action(1, "running", None)
        };
        json!({"steps": [
            {"name": "Spin up environment", "actions": [action(0, "success", out("spin"))]},
            {"name": "Run tests", "actions": [action(0, "success", out("t0")), second]}
        ]})
    }

    fn output(name: &str) -> Value {
        let message = match name {
            "spin" => "Starting container\r\n",
            "t0" => "ok 1\n",
            _ => "not ok 2\nExited with code exit status 1\n",
        };
        json!([{"message": message, "time": "2026-11-01T10:05:01Z", "type": "out"}])
    }

    fn reply(request: &Recorded, server: &str, growth: usize) -> (u16, Value) {
        if let Some(name) = request.path.strip_prefix("/out/") {
            return (200, output(name));
        }
        if request.headers.get("circle-token").map(String::as_str) != Some(TOKEN) {
            return (401, json!({"message": "You must log in first."}));
        }
        let path = request.path.as_str();
        match (request.method.as_str(), path) {
            ("GET", "/api/v2/me") => (200, json!({"login": "ada"})),
            ("GET", "/api/v1.1/projects") => (
                200,
                json!([
                    {"vcs_type": "github", "username": "acme", "reponame": "web",
                        "vcs_url": "https://github.com/acme/web", "default_branch": "main"},
                    {"vcs_type": "bitbucket", "username": "acme", "reponame": "mobile",
                        "vcs_url": "https://bitbucket.org/acme/mobile", "default_branch": "main"},
                    {"vcs_type": "github", "username": "tools", "reponame": "cli",
                        "vcs_url": "https://github.com/tools/cli", "default_branch": "main"}
                ]),
            ),
            ("GET", "/api/v2/project/gh/acme/web") => {
                (200, project("gh/acme/web", "p-web", "web", "main"))
            }
            ("GET", "/api/v2/project/gh/acme/api") => {
                (200, project("gh/acme/api", "p-api", "api", "trunk"))
            }
            ("GET", "/api/v2/project/circleci/org1/proj1") => (
                200,
                project("circleci/org1/proj1", "p-app", "app", "develop"),
            ),
            ("GET", "/api/v2/project/gh/acme/web/pipeline") => {
                (200, page(json!([pipeline(42), pipeline(41)])))
            }
            ("GET", "/api/v2/project/gh/acme/web/pipeline/42") => (200, pipeline(42)),
            ("GET", "/api/v2/project/gh/acme/web/pipeline/41") => (200, pipeline(41)),
            ("GET", "/api/v2/project/gh/acme/api/pipeline") => (200, page(json!([]))),
            ("GET", "/api/v2/project/circleci/org1/proj1/pipeline") => (200, page(json!([]))),
            ("GET", "/api/v2/pipeline/pipe-42/config") => (
                200,
                json!({"source": "version: 2.1\nparameters:\n  deploy:\n    type: boolean\n    default: false\njobs: {}\n"}),
            ),
            ("GET", "/api/v2/projects/p-app/pipeline-definitions") => (
                200,
                page(json!([{"id": "def-a", "name": "build"}, {"id": "def-b", "name": "release"}])),
            ),
            ("GET", "/api/v2/workflow/wf-build-2/job") => (
                200,
                page(json!([
                    {"id": "j1", "name": "checkout", "job_number": 101, "status": "success",
                        "type": "build", "started_at": "2026-11-01T10:05:00Z"},
                    {"id": "j2", "name": "test", "job_number": 102, "status": "running",
                        "type": "build", "started_at": "2026-11-01T10:06:00Z"}
                ])),
            ),
            ("GET", "/api/v2/workflow/wf-deploy/job") => (
                200,
                page(
                    json!([{"id": "j3", "name": "approve", "status": "on_hold", "type": "approval"}]),
                ),
            ),
            ("GET", "/api/v1.1/project/github/acme/web/102") => (200, steps(server, growth)),
            ("POST", "/api/v2/project/gh/acme/web/pipeline") => (
                201,
                json!({"id": "pipe-43", "number": 43, "state": "created"}),
            ),
            ("POST", "/api/v2/project/circleci/org1/proj1/pipeline/run") => (
                201,
                json!({"id": "pipe-7", "number": 7, "state": "created"}),
            ),
            ("POST", path) if path.ends_with("/cancel") => (202, json!({"message": "Accepted."})),
            ("POST", path) if path.ends_with("/rerun") => (202, json!({"workflow_id": "new"})),
            ("GET", path) if path.starts_with("/api/v2/pipeline/") => {
                let id = path
                    .trim_start_matches("/api/v2/pipeline/")
                    .trim_end_matches("/workflow");
                match workflows(id) {
                    Some(items) => (200, page(items)),
                    None => (503, json!({"message": "unavailable"})),
                }
            }
            ("GET", path) if path.starts_with("/api/v2/project/gh/acme/broken") => {
                (503, json!({"message": "unavailable"}))
            }
            (method, path) => (
                404,
                json!({"message": format!("unexpected {method} {path}")}),
            ),
        }
    }

    async fn read_request(stream: &mut tokio::net::TcpStream) -> Option<Recorded> {
        let mut raw = Vec::new();
        let mut buffer = [0; 4096];
        let (head_end, length) = loop {
            let count = stream.read(&mut buffer).await.ok()?;
            if count == 0 {
                return None;
            }
            raw.extend_from_slice(&buffer[..count]);
            if let Some(position) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&raw[..position]).to_lowercase();
                let length = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse().ok())
                    .unwrap_or(0);
                break (position + 4, length);
            }
        };
        while raw.len() < head_end + length {
            let count = stream.read(&mut buffer).await.ok()?;
            if count == 0 {
                return None;
            }
            raw.extend_from_slice(&buffer[..count]);
        }
        let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
        let mut lines = head.lines();
        let mut request_line = lines.next()?.split_whitespace();
        let method = request_line.next()?.to_string();
        let target = request_line.next()?;
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        let headers = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_lowercase(), value.trim().to_string()))
            .collect();
        Some(Recorded {
            method,
            path: path.to_string(),
            query: query.to_string(),
            headers,
            body: serde_json::from_slice(&raw[head_end..head_end + length]).unwrap_or(Value::Null),
        })
    }

    async fn stub(extra: &[(&str, &str)]) -> Stub {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let server_url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Requests::default();
        let growth = Arc::new(AtomicUsize::new(0));
        let (recorded, grown, server) = (requests.clone(), growth.clone(), server_url.clone());
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let (recorded, grown, server) = (recorded.clone(), grown.clone(), server.clone());
                tokio::spawn(async move {
                    let Some(request) = read_request(&mut stream).await else {
                        return;
                    };
                    let (status, body) = reply(&request, &server, grown.load(Ordering::SeqCst));
                    recorded.lock().await.push(request);
                    let body = body.to_string();
                    let response = format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });

        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut pairs = vec![("base_url", server_url.as_str()), ("token", TOKEN)];
        pairs.extend_from_slice(extra);
        let mut plugin = CircleCIPlugin::new();
        plugin
            .initialize(1, cfg(&pairs), Some(Arc::new(reqwest::Client::new())))
            .unwrap();
        Stub {
            plugin,
            requests,
            growth,
        }
    }

    async fn calls(requests: &Requests, method: &str) -> Vec<String> {
        requests
            .lock()
            .await
            .iter()
            .filter(|request| request.method == method)
            .map(|request| match request.query.as_str() {
                "" => request.path.clone(),
                query => format!("{}?{query}", request.path),
            })
            .collect()
    }

    async fn bodies(requests: &Requests, method: &str) -> Vec<Value> {
        requests
            .lock()
            .await
            .iter()
            .filter(|request| request.method == method)
            .map(|request| request.body.clone())
            .collect()
    }

    fn run_ref<'a>(metadata: &'a HashMap<String, Value>, run_number: i64) -> RunRef<'a> {
        RunRef {
            pipeline_id: WEB,
            run_number,
            run_id: "circleci__1__gh/acme/web__pipe-42",
            metadata,
        }
    }

    async fn read(
        plugin: &CircleCIPlugin, node_id: &str, from: Option<&LogCursor>,
    ) -> PluginResult<(String, LogRead, bool)> {
        let metadata = HashMap::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let read = async {
            let mut sink = LogSink::new(tx);
            let result = plugin
                .read_log(run_ref(&metadata, 42), node_id, from, &mut sink)
                .await;
            (result, sink.restarted())
        };
        let collect = async {
            let mut bytes = Vec::new();
            while let Some(chunk) = rx.recv().await {
                bytes.extend_from_slice(&chunk);
            }
            bytes
        };
        let ((result, restarted), bytes) = tokio::join!(read, collect);
        result.map(|read| (String::from_utf8(bytes).unwrap(), read, restarted))
    }

    fn trigger(workflow_id: &str, inputs: Value) -> TriggerParams {
        TriggerParams {
            workflow_id: workflow_id.to_string(),
            inputs: Some(inputs),
        }
    }

    #[tokio::test]
    async fn selected_projects_show_the_status_of_their_newest_pipeline() {
        let stub = stub(&[("selected_items", "gh/acme/web, gh/acme/api")]).await;

        let pipelines = stub.plugin.fetch_pipelines().await.unwrap();

        let summary: Vec<_> = pipelines
            .iter()
            .map(|p| {
                (
                    p.id.as_str(),
                    p.repository.as_str(),
                    p.status,
                    p.branch.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (WEB, "acme/web", PipelineStatus::Running, Some("main")),
                (
                    "circleci__1__gh/acme/api",
                    "acme/api",
                    PipelineStatus::Pending,
                    Some("trunk")
                ),
            ]
        );
        assert_eq!(pipelines[1].last_run, None);
        let gets = calls(&stub.requests, "GET").await;
        let position = |path: &str| gets.iter().position(|get| get == path).unwrap();
        let mut sorted = gets.clone();
        sorted.sort();
        assert_eq!(
            sorted,
            [
                "/api/v2/pipeline/pipe-42/workflow",
                "/api/v2/project/gh/acme/api",
                "/api/v2/project/gh/acme/api/pipeline",
                "/api/v2/project/gh/acme/web",
                "/api/v2/project/gh/acme/web/pipeline",
            ]
        );
        assert!(
            position("/api/v2/project/gh/acme/web/pipeline")
                < position("/api/v2/pipeline/pipe-42/workflow")
        );
    }

    #[tokio::test]
    async fn an_empty_selection_reads_nothing_and_one_failing_project_fails_the_fetch() {
        let stub = stub(&[("selected_items", "")]).await;
        assert!(stub.plugin.fetch_pipelines().await.unwrap().is_empty());
        assert!(stub.requests.lock().await.is_empty());

        let stub = self::stub(&[("selected_items", "gh/acme/web,gh/acme/broken")]).await;
        let error = stub.plugin.fetch_pipelines().await.unwrap_err();
        assert!(
            matches!(error, PluginError::ServiceUnavailable { .. }),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn discovery_groups_followed_projects_by_organization() {
        let stub = stub(&[]).await;

        let organizations: Vec<String> = stub
            .plugin
            .fetch_organizations()
            .await
            .unwrap()
            .into_iter()
            .map(|organization| organization.id)
            .collect();
        assert_eq!(organizations, ["acme", "tools"]);

        let acme: Vec<String> = stub
            .plugin
            .fetch_available_pipelines_filtered(Some("acme".into()), None, None)
            .await
            .unwrap()
            .items
            .into_iter()
            .map(|pipeline| pipeline.id)
            .collect();
        assert_eq!(acme, ["gh/acme/web", "bb/acme/mobile"]);
    }

    #[tokio::test]
    async fn history_runs_take_their_status_from_the_latest_attempt_of_each_workflow() {
        let stub = stub(&[]).await;

        let runs = stub.plugin.fetch_run_history(WEB, 2).await.unwrap();

        let summary: Vec<_> = runs
            .iter()
            .map(|run| (run.run_number, run.status))
            .collect();
        assert_eq!(
            summary,
            [(42, PipelineStatus::Running), (41, PipelineStatus::Failed)]
        );
        assert_eq!(runs[1].metadata["can_rerun_failed_jobs"], json!(true));
        assert_eq!(
            runs[0].logs_url,
            format!("{}/pipelines/github/acme/web/42", stub.plugin.base_url)
        );
    }

    #[tokio::test]
    async fn log_nodes_group_jobs_under_their_workflows() {
        let stub = stub(&[]).await;
        let metadata = HashMap::from([("circleci_pipeline_id".to_string(), json!("pipe-42"))]);

        let nodes = stub
            .plugin
            .fetch_log_nodes(run_ref(&metadata, 42))
            .await
            .unwrap();

        let summary: Vec<_> = nodes
            .iter()
            .map(|node| (node.id.as_str(), node.parent_id.as_deref(), node.log))
            .collect();
        assert_eq!(
            summary,
            [
                ("workflow:wf-build-2", None, None),
                (
                    "101",
                    Some("workflow:wf-build-2"),
                    Some(LogAvailability::Ready)
                ),
                (
                    "102",
                    Some("workflow:wf-build-2"),
                    Some(LogAvailability::Partial)
                ),
                ("workflow:wf-deploy", None, None),
                ("job:j3", Some("workflow:wf-deploy"), None),
            ]
        );
        assert!(!calls(&stub.requests, "GET")
            .await
            .iter()
            .any(|path| path.contains("/pipeline/42")));
    }

    #[tokio::test]
    async fn job_logs_render_steps_as_sections_and_append_finished_steps() {
        let stub = stub(&[]).await;
        let first = "#2026-11-01T10:05:00Z Spin up environment\n.2026-11-01T10:05:01Z Starting container\n#2026-11-01T10:05:00Z Run tests (container 0)\n.2026-11-01T10:05:01Z ok 1\n#2026-11-01T10:05:00Z Run tests (container 1)\n";

        let (output, read_1, restarted) = read(&stub.plugin, "102", None).await.unwrap();
        assert_eq!(output, first);
        assert_eq!(read_1.mode, LogReadMode::Replace);
        assert!(!restarted);
        let outputs: Vec<_> = stub
            .requests
            .lock()
            .await
            .iter()
            .filter(|request| request.path.starts_with("/out/"))
            .map(|request| request.headers.contains_key("circle-token"))
            .collect();
        assert_eq!(outputs, [false, false]);

        stub.growth.store(1, Ordering::SeqCst);
        let (output, read_2, restarted) = read(&stub.plugin, "102", read_1.cursor.as_ref())
            .await
            .unwrap();
        assert_eq!(read_2.mode, LogReadMode::Replace);
        assert!(restarted);
        assert!(output.ends_with(
            "+2026-11-01T10:05:00Z Run tests (container 1)\n.2026-11-01T10:05:01Z not ok 2\n.2026-11-01T10:05:01Z Exited with code exit status 1\n"
        ));

        let (output, read_3, restarted) = read(&stub.plugin, "102", read_2.cursor.as_ref())
            .await
            .unwrap();
        assert_eq!(
            (output.as_str(), read_3.mode, restarted),
            ("", LogReadMode::Append, false)
        );

        assert!(matches!(
            read(&stub.plugin, "workflow:wf-build-2", None).await,
            Err(PluginError::InvalidConfig(_))
        ));
    }

    #[tokio::test]
    async fn job_output_past_the_budget_is_left_out_in_step_order() {
        let mut stub = stub(&[]).await;
        stub.plugin.job_output_limit = "Starting container\r\n".len();

        let (output, _, _) = read(&stub.plugin, "102", None).await.unwrap();

        let omitted = format!(".- {}\n", log_dialect::OMITTED);
        assert_eq!(
            output,
            format!(
                "#2026-11-01T10:05:00Z Spin up environment\n.2026-11-01T10:05:01Z Starting container\n#2026-11-01T10:05:00Z Run tests (container 0)\n{omitted}#2026-11-01T10:05:00Z Run tests (container 1)\n{omitted}"
            )
        );
    }

    #[tokio::test]
    async fn trigger_parameters_list_the_branch_and_the_declared_pipeline_parameters() {
        let stub = stub(&[]).await;

        let parameters = stub.plugin.fetch_workflow_parameters(WEB).await.unwrap();
        let names: Vec<_> = parameters.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["branch", "parameters.deploy"]);
        assert!(matches!(
            &parameters[0].param_type,
            WorkflowParameterType::String { default: Some(branch) } if branch == "main"
        ));

        let app = stub.plugin.fetch_workflow_parameters(APP).await.unwrap();
        let names: Vec<_> = app.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["branch", "pipeline"]);
    }

    #[tokio::test]
    async fn triggering_posts_once_with_the_branch_and_typed_parameters() {
        let stub = stub(&[]).await;

        let response: Value = serde_json::from_str(
            &stub
                .plugin
                .trigger_pipeline(trigger(
                    WEB,
                    json!({"branch": "release", "parameters.deploy": true, "parameters.count": 3.0}),
                ))
                .await
                .unwrap(),
        )
        .unwrap();

        assert_eq!(response["run_number"], json!(43));
        assert_eq!(
            calls(&stub.requests, "POST").await,
            ["/api/v2/project/gh/acme/web/pipeline"]
        );
        assert_eq!(
            bodies(&stub.requests, "POST").await,
            [json!({"branch": "release", "parameters": {"deploy": true, "count": 3}})]
        );
        assert!(calls(&stub.requests, "GET").await.is_empty());
    }

    #[tokio::test]
    async fn app_projects_run_the_chosen_pipeline_definition() {
        let stub = stub(&[]).await;

        let error = stub
            .plugin
            .trigger_pipeline(trigger(APP, json!({})))
            .await
            .unwrap_err();
        assert!(matches!(error, PluginError::InvalidConfig(_)), "{error:?}");
        assert!(calls(&stub.requests, "POST").await.is_empty());

        stub.plugin
            .trigger_pipeline(trigger(APP, json!({"pipeline": "release"})))
            .await
            .unwrap();
        assert_eq!(
            calls(&stub.requests, "POST").await,
            ["/api/v2/project/circleci/org1/proj1/pipeline/run"]
        );
        assert_eq!(
            bodies(&stub.requests, "POST").await,
            [
                json!({"definition_id": "def-b", "config": {"branch": "develop"},
                "checkout": {"branch": "develop"}})
            ]
        );
    }

    #[tokio::test]
    async fn cancel_stops_only_the_active_latest_workflows() {
        let stub = stub(&[]).await;
        stub.plugin.cancel_run(WEB, 42).await.unwrap();
        assert_eq!(
            calls(&stub.requests, "POST").await,
            [
                "/api/v2/workflow/wf-build-2/cancel",
                "/api/v2/workflow/wf-deploy/cancel"
            ]
        );

        let stub = self::stub(&[]).await;
        assert!(stub.plugin.cancel_run(WEB, 41).await.is_err());
        assert!(calls(&stub.requests, "POST").await.is_empty());
    }

    #[tokio::test]
    async fn reruns_target_finished_workflows_and_failed_only_reruns_skip_passing_ones() {
        let stub = stub(&[]).await;
        stub.plugin.rerun_failed_jobs(WEB, 41).await.unwrap();
        stub.plugin.rerun_run(WEB, 41).await.unwrap();
        assert_eq!(
            calls(&stub.requests, "POST").await,
            [
                "/api/v2/workflow/wf-41-build/rerun",
                "/api/v2/workflow/wf-41-build/rerun",
                "/api/v2/workflow/wf-41-lint/rerun"
            ]
        );
        assert_eq!(
            bodies(&stub.requests, "POST").await,
            [
                json!({"from_failed": true}),
                json!({"from_failed": false}),
                json!({"from_failed": false})
            ]
        );

        let stub = self::stub(&[]).await;
        assert!(stub.plugin.rerun_run(WEB, 42).await.is_err());
        assert!(calls(&stub.requests, "POST").await.is_empty());
    }

    #[test]
    fn initialize_requires_a_token_and_a_plain_base_url() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = || Some(Arc::new(reqwest::Client::new()));
        let mut plugin = CircleCIPlugin::new();
        assert!(matches!(
            plugin.initialize(1, cfg(&[]), client()),
            Err(PluginError::InvalidConfig(message)) if message == "Missing token in config"
        ));
        assert!(plugin
            .initialize(
                1,
                cfg(&[
                    ("token", "t"),
                    ("base_url", "https://circleci.acme.dev/api?x=1")
                ]),
                client()
            )
            .is_err());
        plugin
            .initialize(1, cfg(&[("token", "t")]), client())
            .unwrap();
        assert_eq!(plugin.base_url, config::DEFAULT_BASE_URL);
    }
}
