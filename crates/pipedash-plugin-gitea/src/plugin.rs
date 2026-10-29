use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::try_join_all;
use pipedash_plugin_actions::{
    dispatch_body,
    dispatch_inputs,
};
use pipedash_plugin_api::*;

use crate::log_dialect::GiteaLogDialect;
use crate::{
    client,
    config,
    mapper,
    metadata,
    types::JobAttempt,
};

#[derive(serde::Serialize, serde::Deserialize)]
struct LogPosition {
    job: String,
    attempt: JobAttempt,
    offset: u64,
}

pub struct GiteaPlugin {
    metadata: PluginMetadata,
    client: Option<client::GiteaClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
    status_source: Option<StatusSource>,
}

impl Default for GiteaPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl GiteaPlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            client: None,
            provider_id: None,
            config: HashMap::new(),
            status_source: None,
        }
    }

    fn client(&self) -> PluginResult<&client::GiteaClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }

    fn provider_id(&self) -> PluginResult<i64> {
        self.provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))
    }

    async fn fetch_repo_pipelines(
        &self, provider_id: i64, repo_full_name: &str,
    ) -> PluginResult<Vec<Pipeline>> {
        let (owner, repo) = config::parse_repo(repo_full_name)?;
        let client = self.client()?;
        let workflows = client.list_workflows(owner, repo).await?;
        let workflow_ids: Vec<String> = workflows.iter().map(|w| w.id.clone()).collect();
        let latest_runs = client.latest_runs(owner, repo, &workflow_ids).await?;

        Ok(workflows
            .iter()
            .map(|workflow| {
                let latest_run = latest_runs.get(&workflow.id);
                mapper::map_pipeline(provider_id, owner, repo, workflow, latest_run)
            })
            .collect())
    }
}

#[async_trait]
impl Plugin for GiteaPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn status_source(&self) -> Option<&StatusSource> {
        self.status_source.as_ref()
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let base_url = config::get_base_url(&config)?;
        let token = config
            .get("token")
            .map(|token| token.trim())
            .filter(|token| !token.is_empty())
            .ok_or_else(|| PluginError::InvalidConfig("Missing Gitea access token".to_string()))?
            .to_string();
        let status_source = custom_status_source(&config)?;

        let http_client = http_client.unwrap_or_else(|| {
            Arc::new(
                reqwest::Client::builder()
                    .use_rustls_tls()
                    .https_only(cfg!(target_os = "ios"))
                    .pool_max_idle_per_host(10)
                    .timeout(Duration::from_secs(30))
                    .connect_timeout(Duration::from_secs(10))
                    .tcp_keepalive(Duration::from_secs(60))
                    .build()
                    .expect("Failed to build HTTP client"),
            )
        });

        self.status_source = status_source;
        self.client = Some(client::GiteaClient::new(http_client, base_url, token));
        self.provider_id = Some(provider_id);
        self.config = config;
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        self.client()?.get_user().await?;
        Ok(true)
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        self.fetch_available_pipelines_filtered(None, None, params)
            .await
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        self.client()?.list_organizations().await
    }

    async fn ids_with_ci(&self, ids: Vec<String>) -> PluginResult<Vec<String>> {
        let client = self.client()?;
        Ok(retain_with_ci(ids, |id| async move {
            let (owner, repo) = config::parse_repo(&id)?;
            Ok(!client.list_workflows(owner, repo).await?.is_empty())
        })
        .await)
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        let params = params.unwrap_or_default();
        let (uid, exclusive) = match org {
            Some(org) => (
                org.parse().map_err(|_| {
                    PluginError::InvalidConfig(format!("Invalid organization ID: {org}"))
                })?,
                true,
            ),
            None => (client.get_user().await?.id, false),
        };

        let response = client
            .search_repositories(uid, exclusive, search.as_deref(), &params)
            .await?;

        Ok(PaginatedResponse {
            items: response
                .items
                .iter()
                .map(mapper::map_available_pipeline)
                .collect(),
            page: response.page,
            page_size: response.page_size,
            total_count: response.total_count,
            total_pages: response.total_pages,
            has_more: response.has_more,
        })
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self.provider_id()?;
        let repositories = config::get_repositories(&self.config);
        if repositories.is_empty() {
            return Err(PluginError::InvalidConfig(
                "No repositories configured".to_string(),
            ));
        }

        let results = try_join_all(
            repositories
                .iter()
                .map(|repo| self.fetch_repo_pipelines(provider_id, repo)),
        )
        .await?;

        Ok(results.into_iter().flatten().collect())
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let id = config::parse_pipeline_id(pipeline_id)?;
        let runs = self
            .client()?
            .workflow_runs(id.owner, id.repo, id.workflow, limit)
            .await?;
        Ok(runs
            .iter()
            .map(|run| mapper::map_run(run, pipeline_id, id.owner))
            .collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let id = config::parse_pipeline_id(pipeline_id)?;
        let run = self
            .client()?
            .find_run(id.owner, id.repo, id.workflow, run_number)
            .await?;
        Ok(mapper::map_run(&run, pipeline_id, id.owner))
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let id = config::parse_pipeline_id(run.pipeline_id)?;
        let run_id = run
            .metadata
            .get("run_id")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(|| {
                PluginError::InvalidConfig(format!("Run #{} has no Gitea run ID", run.run_number))
            })?;

        let jobs = self
            .client()?
            .list_run_jobs(id.owner, id.repo, run_id)
            .await?;

        Ok(mapper::jobs_to_log_nodes(&jobs))
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let id = config::parse_pipeline_id(run.pipeline_id)?;
        if node_id.is_empty() || !node_id.bytes().all(|b| b.is_ascii_digit()) {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid Gitea job ID: {node_id}"
            )));
        }

        let client = self.client()?;
        let attempt = client.job_attempt(id.owner, id.repo, node_id).await?;
        let position = from
            .and_then(|cursor| cursor.parse::<LogPosition>())
            .filter(|position| position.job == node_id && position.attempt == attempt);
        if from.is_some() && position.is_none() {
            sink.restart()?;
        }
        let (mode, offset) = client
            .stream_job_log(id.owner, id.repo, node_id, position.map(|p| p.offset), sink)
            .await?;
        let job = node_id.to_string();
        Ok(LogRead {
            mode,
            cursor: Some(LogCursor::new(&LogPosition {
                job,
                attempt,
                offset,
            })?),
        })
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &GiteaLogDialect
    }

    async fn fetch_workflow_parameters(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        let id = config::parse_pipeline_id(workflow_id)?;
        let client = self.client()?;
        let workflows = client.list_workflows(id.owner, id.repo).await?;
        let path = workflows
            .iter()
            .find(|workflow| workflow.id == id.workflow)
            .map(|workflow| workflow.path.as_str())
            .ok_or_else(|| {
                PluginError::PipelineNotFound(format!("Workflow {} not found", id.workflow))
            })?;
        let source = client.raw_file(id.owner, id.repo, path).await?;

        let mut parameters = vec![WorkflowParameter {
            name: "ref".to_string(),
            label: Some("Ref".to_string()),
            description: Some("Branch, tag, or commit SHA to run workflow on".to_string()),
            param_type: WorkflowParameterType::String {
                default: Some("main".to_string()),
            },
            required: true,
        }];
        parameters.extend(dispatch_inputs(&source)?);
        Ok(parameters)
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let id = config::parse_pipeline_id(&params.workflow_id)?;
        let client = self.client()?;

        let body = dispatch_body(params.inputs);

        client
            .dispatch_workflow(id.owner, id.repo, id.workflow, &body)
            .await?;
        Ok(client.actions_url(id.owner, id.repo))
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let id = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        let run = client
            .find_run(id.owner, id.repo, id.workflow, run_number)
            .await?;
        client.cancel_run(id.owner, id.repo, run.id).await
    }
}

#[cfg(all(test, not(target_os = "ios")))]
mod tests {
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };

    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };

    use super::*;

    const STUB_REPOS: usize = 150;
    const STUB_ORGS: usize = 70;
    const MAX_RESPONSE_ITEMS: usize = 50;
    const DEFAULT_PAGING_NUM: usize = 30;

    fn stub_page(url: &reqwest::Url, total: usize) -> std::ops::Range<usize> {
        let query = |key: &str| {
            url.query_pairs()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.parse::<usize>().unwrap())
        };
        let limit = query("limit")
            .unwrap_or(DEFAULT_PAGING_NUM)
            .min(MAX_RESPONSE_ITEMS);
        let page = query("page").unwrap_or(1);
        ((page - 1) * limit).min(total)..(page * limit).min(total)
    }

    const JOB_LOG: &str = "2026-10-28T10:00:01.1000000Z ::group::Set up job\n\
                           2026-10-28T10:00:02.2000000Z cloning into /workspace\n\
                           2026-10-28T10:00:03.3000000Z ::endgroup::\n";

    fn stub_jobs(page: usize) -> serde_json::Value {
        let step = |number: i64, name: &str, status: &str, conclusion: &str| {
            let completed_at = if status == "completed" {
                format!("2026-10-28T10:00:0{}Z", number + 2)
            } else {
                "1970-01-01T00:00:00Z".to_string()
            };
            serde_json::json!({
                "name": name, "number": number, "status": status, "conclusion": conclusion,
                "started_at": format!("2026-10-28T10:00:0{}Z", number + 1),
                "completed_at": completed_at
            })
        };
        let job = |id: i64, name: &str, status: &str, conclusion: &str, started: &str| {
            serde_json::json!({
                "id": id, "name": name, "status": status, "conclusion": conclusion,
                "html_url": format!("http://g/claude/smoke__ci/actions/runs/7/jobs/{}", id - 101),
                "started_at": started, "completed_at": "1970-01-01T00:00:00Z",
                "steps": null
            })
        };
        let unset = "1970-01-01T00:00:00Z";
        let mut build = job(101, "build", "completed", "success", "2026-10-28T10:00:00Z");
        build["steps"] = serde_json::json!([
            step(0, "Set up job", "completed", "success"),
            step(1, "Run tests", "completed", "success")
        ]);
        let mut lint = job(102, "lint", "in_progress", "", "2026-10-28T10:00:00Z");
        lint["steps"] = serde_json::json!([step(0, "Set up job", "in_progress", "")]);
        let jobs = match page {
            1 => vec![build, lint],
            2 => vec![
                job(103, "deploy", "queued", "", unset),
                job(104, "publish", "completed", "skipped", unset),
            ],
            _ => Vec::new(),
        };
        serde_json::json!({"jobs": jobs, "total_count": 4})
    }

    async fn stub_gitea() -> (GiteaPlugin, tokio::task::JoinHandle<()>) {
        let (plugin, server, _) = stub_gitea_counting().await;
        (plugin, server)
    }

    async fn stub_gitea_counting() -> (GiteaPlugin, tokio::task::JoinHandle<()>, Arc<AtomicUsize>) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let requests = Arc::new(AtomicUsize::new(0));
        let counter = requests.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    let count = stream.read(&mut buffer).await.unwrap();
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..count]);
                }
                let request = String::from_utf8(request).unwrap();
                counter.fetch_add(1, Ordering::SeqCst);
                let mut head = request.split_whitespace();
                let method = head.next().unwrap();
                let url =
                    reqwest::Url::parse(&format!("http://{addr}{}", head.next().unwrap())).unwrap();
                let mut total_count = None;
                let (status, body) = match (method, url.path()) {
                    ("GET", "/api/v1/repos/search") => {
                        total_count = Some(STUB_REPOS);
                        let data: Vec<_> = stub_page(&url, STUB_REPOS)
                            .map(|i| {
                                serde_json::json!({
                                    "name": format!("r{i}"),
                                    "full_name": format!("org/r{i}"),
                                    "owner": {"login": "org"}
                                })
                            })
                            .collect();
                        (200, serde_json::json!({"data": data}))
                    }
                    ("GET", "/api/v1/user/orgs") => {
                        total_count = Some(STUB_ORGS);
                        let orgs: Vec<_> = stub_page(&url, STUB_ORGS)
                            .map(|i| serde_json::json!({"id": 100 + i, "username": format!("o{i}")}))
                            .collect();
                        (200, serde_json::json!(orgs))
                    }
                    ("GET", "/api/v1/user") => {
                        (200, serde_json::json!({"id": 2, "login": "claude"}))
                    }
                    ("GET", "/api/v1/repos/claude/smoke__ci/actions/workflows") => (
                        200,
                        serde_json::json!({"workflows": [
                            {"id": "build__test.yaml", "name": "Build", "path": ".gitea/workflows/build__test.yaml"},
                            {"id": "lint.yaml", "name": "Lint", "path": ".gitea/workflows/lint.yaml"}
                        ]}),
                    ),
                    ("GET", "/api/v1/repos/claude/smoke__ci/actions/runs") => {
                        let page: usize = url
                            .query_pairs()
                            .find(|(key, _)| key == "page")
                            .map(|(_, value)| value.parse().unwrap())
                            .unwrap();
                        let runs: Vec<_> = if page == 1 {
                            (0..50)
                            .map(|i| {
                                serde_json::json!({
                                    "id": 100 - i, "run_number": 100 - i, "status": "queued",
                                    "path": "build__test.yaml@refs/heads/main", "head_branch": "main",
                                    "html_url": format!("http://g/claude/smoke__ci/actions/runs/{}", 100 - i)
                                })
                            })
                            .collect()
                        } else {
                            vec![
                                serde_json::json!({"id": 3, "run_number": 3, "status": "completed", "conclusion": "failure",
                             "path": "lint.yaml@refs/heads/main", "head_branch": "main",
                             "html_url": "http://g/claude/smoke__ci/actions/runs/3"}),
                                serde_json::json!({"id": 2, "run_number": 2, "status": "queued",
                             "path": "build__test.yaml@refs/heads/main", "head_branch": "main",
                             "html_url": "http://g/claude/smoke__ci/actions/runs/2"}),
                                serde_json::json!({"id": 1, "run_number": 1, "status": "completed", "conclusion": "success",
                             "path": "build__test.yaml@refs/heads/main", "head_branch": "main",
                             "html_url": "http://g/claude/smoke__ci/actions/runs/1"}),
                            ]
                        };
                        (200, serde_json::json!({"workflow_runs": runs}))
                    }
                    ("POST", "/api/v1/repos/claude/smoke__ci/actions/runs/2/cancel") => {
                        (404, serde_json::json!({"message": "not found"}))
                    }
                    ("GET", "/api/v1/repos/claude/missing/actions/workflows") => {
                        (404, serde_json::json!({"message": "not found"}))
                    }
                    ("GET", "/api/v1/repos/claude/empty/actions/workflows") => {
                        (200, serde_json::json!({"workflows": []}))
                    }
                    ("GET", "/api/v1/repos/claude/smoke__ci/raw/.gitea/workflows/build__test.yaml") => {
                        (200, serde_json::json!("on:\n  workflow_dispatch:\n    inputs:\n      greeting:\n        description: Greeting\n        default: hi\n"))
                    }
                    ("GET", "/api/v1/repos/claude/smoke__ci/actions/runs/7/jobs") => {
                        let page = url
                            .query_pairs()
                            .find(|(key, _)| key == "page")
                            .map(|(_, value)| value.parse().unwrap())
                            .unwrap();
                        (200, stub_jobs(page))
                    }
                    ("GET", "/api/v1/repos/claude/smoke__ci/actions/jobs/101") => (
                        200,
                        serde_json::json!({"id": 101, "name": "build", "status": "completed", "run_attempt": 1, "started_at": "2026-10-28T10:00:00Z"}),
                    ),
                    ("GET", "/api/v1/repos/claude/smoke__ci/actions/jobs/104") => (
                        200,
                        serde_json::json!({"id": 104, "name": "deploy", "status": "waiting"}),
                    ),
                    ("GET", "/api/v1/repos/claude/smoke__ci/actions/jobs/101/logs") => {
                        (200, serde_json::json!(JOB_LOG))
                    }
                    ("GET", "/api/v1/repos/claude/smoke__ci/actions/jobs/104/logs") => {
                        (404, serde_json::json!({"message": "job not started"}))
                    }
                    ("GET", "/api/v1/repos/claude/old/actions/runs/7/jobs") => {
                        (404, serde_json::json!({"message": "not found"}))
                    }
                    (method, path) => panic!("Unexpected stub request: {method} {path}"),
                };
                let body = match body {
                    serde_json::Value::String(raw) => raw,
                    json => json.to_string(),
                };
                let total_header = total_count
                    .map(|total| format!("X-Total-Count: {total}\r\n"))
                    .unwrap_or_default();
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n{total_header}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let mut plugin = GiteaPlugin::new();
        plugin
            .initialize(
                1,
                HashMap::from([
                    ("base_url".to_string(), format!("http://{addr}/")),
                    ("token".to_string(), "stub-token".to_string()),
                    ("selected_items".to_string(), "claude/smoke__ci".to_string()),
                ]),
                None,
            )
            .unwrap();
        (plugin, server, requests)
    }

    #[tokio::test]
    async fn repositories_without_workflows_are_dropped_from_a_selection() {
        let (plugin, server) = stub_gitea().await;

        let kept = plugin
            .ids_with_ci(
                ["claude/smoke__ci", "claude/empty", "claude/missing"]
                    .map(String::from)
                    .to_vec(),
            )
            .await
            .unwrap();
        server.abort();

        assert_eq!(kept, ["claude/smoke__ci", "claude/missing"]);
    }

    #[tokio::test]
    async fn pipelines_find_latest_run_of_each_workflow_beyond_first_page() {
        let (plugin, server) = stub_gitea().await;
        let pipelines = plugin.fetch_pipelines().await.unwrap();

        let summary: Vec<_> = pipelines
            .iter()
            .map(|p| (p.id.as_str(), p.status))
            .collect();
        assert_eq!(
            summary,
            [
                (
                    "gitea__1__claude/smoke__ci/build__test.yaml",
                    PipelineStatus::Running
                ),
                (
                    "gitea__1__claude/smoke__ci/lint.yaml",
                    PipelineStatus::Failed
                ),
            ]
        );
        server.abort();
    }

    #[tokio::test]
    async fn one_failing_repository_fails_the_whole_fetch() {
        let (mut plugin, server) = stub_gitea().await;
        plugin.config.insert(
            "selected_items".to_string(),
            "claude/smoke__ci,claude/missing".to_string(),
        );
        let error = plugin.fetch_pipelines().await.unwrap_err();
        assert!(matches!(error, PluginError::PipelineNotFound(_)), "{error}");
        server.abort();
    }

    #[tokio::test]
    async fn workflow_parameters_include_dispatch_inputs_after_ref() {
        let (plugin, server) = stub_gitea().await;
        let params = plugin
            .fetch_workflow_parameters("gitea__1__claude/smoke__ci/build__test.yaml")
            .await
            .unwrap();
        assert_eq!(
            params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["ref", "inputs.greeting"]
        );
        assert_eq!(params[1].label.as_deref(), Some("greeting"));
        server.abort();
    }

    #[tokio::test]
    async fn run_history_filters_by_workflow_and_respects_limit() {
        let (plugin, server) = stub_gitea().await;
        let runs = plugin
            .fetch_run_history("gitea__1__claude/smoke__ci/build__test.yaml", 1)
            .await
            .unwrap();

        assert_eq!(runs.iter().map(|r| r.run_number).collect::<Vec<_>>(), [100]);
        let details = plugin
            .fetch_run_details("gitea__1__claude/smoke__ci/build__test.yaml", 1)
            .await
            .unwrap();
        assert_eq!(details.status, PipelineStatus::Success);
        assert!(plugin
            .fetch_run_details("gitea__1__claude/smoke__ci/build__test.yaml", 3)
            .await
            .is_err());
        server.abort();
    }

    #[tokio::test]
    async fn cancel_reports_not_supported_when_endpoint_is_missing() {
        let (plugin, server) = stub_gitea().await;
        let error = plugin
            .cancel_run("gitea__1__claude/smoke__ci/build__test.yaml", 2)
            .await
            .unwrap_err();
        assert!(matches!(error, PluginError::NotSupported(_)), "{error}");
        server.abort();
    }

    #[tokio::test]
    async fn dispatch_is_not_resent_after_a_server_error() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = requests.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buffer = [0; 4096];
                let _ = stream.read(&mut buffer).await.unwrap();
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                stream
                    .write_all(b"HTTP/1.1 503 X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await
                    .unwrap();
            }
        });
        let mut plugin = GiteaPlugin::new();
        plugin
            .initialize(
                1,
                HashMap::from([
                    ("base_url".to_string(), format!("http://{addr}/")),
                    ("token".to_string(), "stub-token".to_string()),
                ]),
                None,
            )
            .unwrap();

        let error = plugin
            .client()
            .unwrap()
            .dispatch_workflow("o", "r", "w.yaml", &serde_json::json!({}))
            .await
            .unwrap_err();
        server.abort();

        assert!(
            matches!(error, PluginError::ServiceUnavailable { .. }),
            "{error}"
        );
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn repo_picker_pages_reach_every_repo_when_gitea_caps_the_limit() {
        let (plugin, server) = stub_gitea().await;
        let mut names = Vec::new();
        let mut has_more = true;
        let mut page = 1;
        while has_more {
            let response = plugin
                .fetch_available_pipelines_filtered(
                    Some("7".into()),
                    None,
                    Some(PaginationParams {
                        page,
                        page_size: 100,
                    }),
                )
                .await
                .unwrap();
            names.extend(response.items.into_iter().map(|item| item.name));
            has_more = response.has_more;
            page += 1;
        }
        server.abort();

        let expected: Vec<_> = (0..STUB_REPOS).map(|i| format!("org/r{i}")).collect();
        assert_eq!(names, expected);
    }

    #[tokio::test]
    async fn organizations_include_every_page() {
        let (plugin, server) = stub_gitea().await;
        let orgs = plugin.fetch_organizations().await.unwrap();
        server.abort();

        assert_eq!(orgs.len(), STUB_ORGS + 1);
        assert_eq!(orgs.last().unwrap().name, format!("o{}", STUB_ORGS - 1));
    }

    fn run_metadata() -> HashMap<String, serde_json::Value> {
        HashMap::from([("run_id".to_string(), serde_json::json!(7))])
    }

    fn run_7<'a>(
        pipeline_id: &'a str, metadata: &'a HashMap<String, serde_json::Value>,
    ) -> RunRef<'a> {
        RunRef {
            pipeline_id,
            run_number: 3,
            run_id: "gitea-run-7",
            metadata,
        }
    }

    const SMOKE_PIPELINE: &str = "gitea__1__claude/smoke__ci/build__test.yaml";

    async fn read(plugin: &GiteaPlugin, node_id: &str) -> PluginResult<Vec<u8>> {
        let metadata = run_metadata();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let mut sink = LogSink::new(tx);
        let result = plugin
            .read_log(run_7(SMOKE_PIPELINE, &metadata), node_id, None, &mut sink)
            .await;
        drop(sink);
        let mut bytes = Vec::new();
        while let Some(chunk) = rx.recv().await {
            bytes.extend_from_slice(&chunk);
        }
        result.map(|_| bytes)
    }

    #[tokio::test]
    async fn run_jobs_and_steps_become_log_nodes_across_pages() {
        let (plugin, server, requests) = stub_gitea_counting().await;
        let metadata = run_metadata();

        let nodes = plugin
            .fetch_log_nodes(run_7(SMOKE_PIPELINE, &metadata))
            .await
            .unwrap();
        server.abort();

        let shape: Vec<_> = nodes
            .iter()
            .map(|node| {
                (
                    node.id.as_str(),
                    node.parent_id.as_deref(),
                    node.kind,
                    node.status,
                    node.log,
                )
            })
            .collect();
        assert_eq!(
            shape,
            [
                (
                    "101",
                    None,
                    LogNodeKind::Job,
                    PipelineStatus::Success,
                    Some(LogAvailability::Ready)
                ),
                (
                    "101:0",
                    Some("101"),
                    LogNodeKind::Step,
                    PipelineStatus::Success,
                    None
                ),
                (
                    "101:1",
                    Some("101"),
                    LogNodeKind::Step,
                    PipelineStatus::Success,
                    None
                ),
                (
                    "102",
                    None,
                    LogNodeKind::Job,
                    PipelineStatus::Running,
                    Some(LogAvailability::Partial)
                ),
                (
                    "102:0",
                    Some("102"),
                    LogNodeKind::Step,
                    PipelineStatus::Running,
                    None
                ),
                (
                    "103",
                    None,
                    LogNodeKind::Job,
                    PipelineStatus::Running,
                    Some(LogAvailability::Pending)
                ),
                ("104", None, LogNodeKind::Job, PipelineStatus::Skipped, None),
            ]
        );
        assert_eq!(nodes[0].name, "build");
        assert_eq!(nodes[2].name, "Run tests");
        assert_eq!(
            nodes[0].web_url.as_deref(),
            Some("http://g/claude/smoke__ci/actions/runs/7/jobs/0")
        );
        assert_eq!(requests.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn job_log_streams_the_text_gitea_stores() {
        let (plugin, server) = stub_gitea().await;

        let bytes = read(&plugin, "101").await.unwrap();
        server.abort();

        assert_eq!(bytes, JOB_LOG.as_bytes());
    }

    #[tokio::test]
    async fn stored_log_lines_reach_the_dialect_as_sections() {
        let (plugin, server) = stub_gitea().await;

        let bytes = read(&plugin, "101").await.unwrap();
        server.abort();

        let markers: Vec<_> = bytes
            .split(|b| *b == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| plugin.log_dialect().parse(line).marker)
            .collect();
        assert_eq!(
            markers,
            [
                LineMarker::GroupStart,
                LineMarker::Plain,
                LineMarker::GroupEnd
            ]
        );
    }

    #[tokio::test]
    async fn job_that_never_started_has_no_log() {
        let (plugin, server) = stub_gitea().await;

        let error = read(&plugin, "104").await.unwrap_err();
        server.abort();

        assert!(
            matches!(error, PluginError::PipelineNotFound(_)),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn node_ids_that_are_not_job_numbers_are_rejected_without_a_request() {
        let (plugin, server, requests) = stub_gitea_counting().await;

        for node_id in [
            "",
            "101:1",
            "../runs/7",
            "101/logs",
            "x?y=1",
            "+101",
            "-1",
            "1 2",
        ] {
            let error = read(&plugin, node_id).await.unwrap_err();
            assert!(
                matches!(error, PluginError::InvalidConfig(_)),
                "{node_id}: {error:?}"
            );
        }
        server.abort();

        assert_eq!(requests.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn run_without_a_gitea_run_id_is_rejected_without_a_request() {
        let (plugin, server, requests) = stub_gitea_counting().await;
        let metadata = HashMap::new();

        let error = plugin
            .fetch_log_nodes(run_7(SMOKE_PIPELINE, &metadata))
            .await
            .unwrap_err();
        server.abort();

        assert!(matches!(error, PluginError::InvalidConfig(_)), "{error:?}");
        assert_eq!(requests.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn gitea_without_the_jobs_route_reports_the_minimum_version() {
        let (plugin, server) = stub_gitea().await;
        let metadata = run_metadata();

        let error = plugin
            .fetch_log_nodes(run_7("gitea__1__claude/old/ci.yaml", &metadata))
            .await
            .unwrap_err();
        server.abort();

        let PluginError::NotSupported(message) = error else {
            panic!("expected NotSupported, got {error:?}");
        };
        assert!(message.contains("Gitea 1.25 or newer"), "{message}");
    }

    #[derive(Clone, Copy)]
    enum RangeSupport {
        Honored,
        Ignored,
        Misaligned,
    }

    const STARTED_AT: &str = "2026-10-28T10:00:00Z";

    struct Log {
        body: Vec<u8>,
        range_support: RangeSupport,
        ranges: Vec<Option<String>>,
        run_attempt: i64,
        started_at: &'static str,
    }

    type SharedLog = Arc<std::sync::Mutex<Log>>;

    async fn stub_log(
        body: &[u8], range_support: RangeSupport,
    ) -> (GiteaPlugin, SharedLog, tokio::task::JoinHandle<()>) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let log = SharedLog::new(std::sync::Mutex::new(Log {
            body: body.to_vec(),
            range_support,
            ranges: Vec::new(),
            run_attempt: 1,
            started_at: STARTED_AT,
        }));
        let state = log.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    let count = stream.read(&mut buffer).await.unwrap();
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..count]);
                }
                let request = String::from_utf8(request).unwrap();
                let range = request.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("range")
                        .then(|| value.trim().to_string())
                });
                let target = request.split_whitespace().nth(1).unwrap().to_string();
                let (status, content_range, payload) = {
                    let mut log = state.lock().unwrap();
                    if target.ends_with("/logs") {
                        log.ranges.push(range.clone());
                        let total = log.body.len();
                        let start = range.as_deref().and_then(|value| {
                            value
                                .strip_prefix("bytes=")?
                                .strip_suffix('-')?
                                .parse()
                                .ok()
                        });
                        match (start, log.range_support) {
                            (Some(start), RangeSupport::Honored) if start >= total => (
                                "416 Range Not Satisfiable",
                                format!("Content-Range: bytes */{total}\r\n"),
                                Vec::new(),
                            ),
                            (Some(start), RangeSupport::Honored) => (
                                "206 Partial Content",
                                format!("Content-Range: bytes {start}-{}/{total}\r\n", total - 1),
                                log.body[start..].to_vec(),
                            ),
                            (Some(_), RangeSupport::Misaligned) => (
                                "206 Partial Content",
                                format!("Content-Range: bytes 0-{}/{total}\r\n", total - 1),
                                log.body.clone(),
                            ),
                            _ => ("200 OK", String::new(), log.body.clone()),
                        }
                    } else {
                        let job = serde_json::json!({
                            "id": 101,
                            "name": "build",
                            "status": "in_progress",
                            "run_attempt": log.run_attempt,
                            "started_at": log.started_at
                        });
                        ("200 OK", String::new(), job.to_string().into_bytes())
                    }
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\n{content_range}Content-Length: {}\r\nConnection: close\r\n\r\n",
                    payload.len()
                );
                stream.write_all(head.as_bytes()).await.unwrap();
                stream.write_all(&payload).await.unwrap();
            }
        });
        let mut plugin = GiteaPlugin::new();
        plugin
            .initialize(
                1,
                HashMap::from([
                    ("base_url".to_string(), format!("http://{addr}/")),
                    ("token".to_string(), "stub-token".to_string()),
                ]),
                None,
            )
            .unwrap();
        (plugin, log, server)
    }

    fn cursor(job: &str, offset: u64) -> LogCursor {
        cursor_of(job, 1, STARTED_AT, offset)
    }

    fn cursor_of(job: &str, run: i64, started_at: &str, offset: u64) -> LogCursor {
        LogCursor::new(&serde_json::json!({
            "job": job,
            "attempt": {"run": run, "started_at": started_at},
            "offset": offset
        }))
        .unwrap()
    }

    async fn read_log(
        plugin: &GiteaPlugin, from: Option<&LogCursor>,
    ) -> (PluginResult<LogRead>, Vec<u8>, bool) {
        let metadata = run_metadata();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let mut sink = LogSink::new(tx);
        let result = plugin
            .read_log(run_7(SMOKE_PIPELINE, &metadata), "101", from, &mut sink)
            .await;
        let restarted = sink.restarted();
        drop(sink);
        let mut bytes = Vec::new();
        while let Some(chunk) = rx.recv().await {
            bytes.extend_from_slice(&chunk);
        }
        (result, bytes, restarted)
    }

    fn ranges(log: &SharedLog) -> Vec<Option<String>> {
        log.lock().unwrap().ranges.clone()
    }

    #[tokio::test]
    async fn resumed_log_appends_only_the_new_bytes() {
        let (plugin, log, server) = stub_log(b"a\nb\n", RangeSupport::Honored).await;

        let (first, bytes, restarted) = read_log(&plugin, None).await;
        assert_eq!(first.unwrap(), LogRead::replace(Some(cursor("101", 4))));
        assert_eq!(bytes, b"a\nb\n");
        assert!(!restarted);

        log.lock().unwrap().body.extend_from_slice(b"c\n");
        let (second, bytes, restarted) = read_log(&plugin, Some(&cursor("101", 4))).await;
        assert_eq!(second.unwrap(), LogRead::append(cursor("101", 6)));
        assert_eq!(bytes, b"c\n");
        assert!(!restarted);

        let (idle, bytes, restarted) = read_log(&plugin, Some(&cursor("101", 6))).await;
        server.abort();
        assert_eq!(idle.unwrap(), LogRead::append(cursor("101", 6)));
        assert!(bytes.is_empty() && !restarted);
        assert_eq!(
            ranges(&log),
            [None, Some("bytes=4-".into()), Some("bytes=6-".into())]
        );
    }

    #[tokio::test]
    async fn server_ignoring_range_restarts_with_the_whole_log() {
        let (plugin, log, server) = stub_log(b"a\nb\nc\n", RangeSupport::Ignored).await;

        let (read, bytes, restarted) = read_log(&plugin, Some(&cursor("101", 4))).await;
        server.abort();

        assert_eq!(read.unwrap(), LogRead::replace(Some(cursor("101", 6))));
        assert_eq!(bytes, b"a\nb\nc\n");
        assert!(restarted);
        assert_eq!(ranges(&log), [Some("bytes=4-".into())]);
    }

    #[tokio::test]
    async fn partial_reply_starting_elsewhere_restarts_with_the_whole_log() {
        let (plugin, log, server) = stub_log(b"a\nb\nc\n", RangeSupport::Misaligned).await;

        let (read, bytes, restarted) = read_log(&plugin, Some(&cursor("101", 4))).await;
        server.abort();

        assert_eq!(read.unwrap(), LogRead::replace(Some(cursor("101", 6))));
        assert_eq!(bytes, b"a\nb\nc\n");
        assert!(restarted);
        assert_eq!(ranges(&log), [Some("bytes=4-".into()), None]);
    }

    #[tokio::test]
    async fn shrunk_log_restarts_with_the_whole_log() {
        let (plugin, log, server) = stub_log(b"a\n", RangeSupport::Honored).await;

        let (read, bytes, restarted) = read_log(&plugin, Some(&cursor("101", 6))).await;
        server.abort();

        assert_eq!(read.unwrap(), LogRead::replace(Some(cursor("101", 2))));
        assert_eq!(bytes, b"a\n");
        assert!(restarted);
        assert_eq!(ranges(&log), [Some("bytes=6-".into()), None]);
    }

    #[tokio::test]
    async fn unusable_cursors_restart_without_a_range_request() {
        let (plugin, log, server) = stub_log(b"a\nb\n", RangeSupport::Honored).await;

        for stale in [LogCursor::new(&"garbage").unwrap(), cursor("102", 4)] {
            let (read, bytes, restarted) = read_log(&plugin, Some(&stale)).await;
            assert_eq!(read.unwrap(), LogRead::replace(Some(cursor("101", 4))));
            assert_eq!(bytes, b"a\nb\n");
            assert!(restarted);
        }
        server.abort();

        assert_eq!(ranges(&log), [None, None]);
    }

    #[tokio::test]
    async fn rerun_keeping_the_job_id_restarts_even_when_its_log_is_longer_than_the_cursor() {
        let (plugin, log, server) = stub_log(b"a\nb\n", RangeSupport::Honored).await;

        let (first, _, _) = read_log(&plugin, None).await;
        let first = first.unwrap();
        assert_eq!(first, LogRead::replace(Some(cursor("101", 4))));

        for (run, started_at) in [(2, STARTED_AT), (1, "2026-10-28T11:00:00Z")] {
            {
                let mut log = log.lock().unwrap();
                log.run_attempt = run;
                log.started_at = started_at;
                log.body = b"x\ny\nz\nw\n".to_vec();
            }
            let (read, bytes, restarted) = read_log(&plugin, first.cursor.as_ref()).await;
            assert_eq!(
                read.unwrap(),
                LogRead::replace(Some(cursor_of("101", run, started_at, 8)))
            );
            assert_eq!(bytes, b"x\ny\nz\nw\n");
            assert!(restarted);
        }
        server.abort();

        assert_eq!(ranges(&log), [None, None, None]);
    }
}
