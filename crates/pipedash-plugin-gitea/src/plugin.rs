use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;
use pipedash_plugin_api::*;

use crate::{
    client,
    config,
    mapper,
    metadata,
};

pub struct GiteaPlugin {
    metadata: PluginMetadata,
    client: Option<client::GiteaClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
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
        let (workflows, runs) = futures::try_join!(
            client.list_workflows(owner, repo),
            client.list_runs(owner, repo, 1)
        )?;

        Ok(workflows
            .iter()
            .map(|workflow| {
                let latest_run = runs.iter().find(|run| run.workflow_file() == workflow.id);
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

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let token = config
            .get("token")
            .map(|token| token.trim())
            .filter(|token| !token.is_empty())
            .ok_or_else(|| PluginError::InvalidConfig("Missing Gitea access token".to_string()))?
            .to_string();
        let base_url = config::get_base_url(&config)?;

        let http_client = http_client.unwrap_or_else(|| {
            Arc::new(
                reqwest::Client::builder()
                    .use_rustls_tls()
                    .pool_max_idle_per_host(10)
                    .timeout(Duration::from_secs(30))
                    .connect_timeout(Duration::from_secs(10))
                    .tcp_keepalive(Duration::from_secs(60))
                    .build()
                    .expect("Failed to build HTTP client"),
            )
        });

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

        let results = join_all(
            repositories
                .iter()
                .map(|repo| self.fetch_repo_pipelines(provider_id, repo)),
        )
        .await;

        let mut pipelines = Vec::new();
        let mut first_error = None;
        for result in results {
            match result {
                Ok(repo_pipelines) => pipelines.extend(repo_pipelines),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to fetch Gitea workflows");
                    first_error.get_or_insert(e);
                }
            }
        }

        match first_error {
            Some(e) if pipelines.is_empty() => Err(e),
            _ => Ok(pipelines),
        }
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

    async fn fetch_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        Ok(vec![WorkflowParameter {
            name: "ref".to_string(),
            label: Some("Ref".to_string()),
            description: Some("Branch, tag, or commit SHA to run workflow on".to_string()),
            param_type: WorkflowParameterType::String {
                default: Some("main".to_string()),
            },
            required: true,
        }])
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let id = config::parse_pipeline_id(&params.workflow_id)?;
        let client = self.client()?;

        let mut inputs = params
            .inputs
            .and_then(|inputs| inputs.as_object().cloned())
            .unwrap_or_default();
        let ref_value = inputs
            .remove("ref")
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_else(|| "main".to_string());

        let mut body = serde_json::json!({ "ref": ref_value });
        if !inputs.is_empty() {
            body["inputs"] = serde_json::Value::Object(inputs);
        }

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

#[cfg(test)]
mod tests {
    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };

    use super::*;

    async fn stub_gitea() -> (GiteaPlugin, tokio::task::JoinHandle<()>) {
        let _ = rustls::crypto::ring::default_provider().install_default();
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
                let mut head = request.split_whitespace();
                let method = head.next().unwrap();
                let url =
                    reqwest::Url::parse(&format!("http://{addr}{}", head.next().unwrap())).unwrap();
                let (status, body) = match (method, url.path()) {
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
                    ("GET", "/api/v1/repos/claude/smoke__ci/actions/runs") => (
                        200,
                        serde_json::json!({"workflow_runs": [
                            {"id": 3, "run_number": 3, "status": "completed", "conclusion": "failure",
                             "path": "lint.yaml@refs/heads/main", "head_branch": "main",
                             "html_url": "http://g/claude/smoke__ci/actions/runs/3"},
                            {"id": 2, "run_number": 2, "status": "queued",
                             "path": "build__test.yaml@refs/heads/main", "head_branch": "main",
                             "html_url": "http://g/claude/smoke__ci/actions/runs/2"},
                            {"id": 1, "run_number": 1, "status": "completed", "conclusion": "success",
                             "path": "build__test.yaml@refs/heads/main", "head_branch": "main",
                             "html_url": "http://g/claude/smoke__ci/actions/runs/1"}
                        ]}),
                    ),
                    ("POST", "/api/v1/repos/claude/smoke__ci/actions/runs/2/cancel") => {
                        (404, serde_json::json!({"message": "not found"}))
                    }
                    (method, path) => panic!("Unexpected stub request: {method} {path}"),
                };
                let body = body.to_string();
                let response = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
        (plugin, server)
    }

    #[tokio::test]
    async fn pipelines_take_latest_run_of_their_own_workflow() {
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
    async fn run_history_filters_by_workflow_and_respects_limit() {
        let (plugin, server) = stub_gitea().await;
        let runs = plugin
            .fetch_run_history("gitea__1__claude/smoke__ci/build__test.yaml", 1)
            .await
            .unwrap();

        assert_eq!(runs.iter().map(|r| r.run_number).collect::<Vec<_>>(), [2]);
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
}
