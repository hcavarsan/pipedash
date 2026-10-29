use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;
use pipedash_plugin_api::*;

use crate::log_dialect::BitbucketLogDialect;
use crate::{
    client,
    config,
    mapper,
    metadata,
    types,
};

#[derive(serde::Serialize, serde::Deserialize)]
struct LogPosition {
    pipeline: String,
    step: String,
    offset: u64,
}

pub struct BitbucketPlugin {
    metadata: PluginMetadata,
    client: Option<client::BitbucketClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
    status_source: Option<StatusSource>,
}

impl Default for BitbucketPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl BitbucketPlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            client: None,
            provider_id: None,
            config: HashMap::new(),
            status_source: None,
        }
    }

    fn client(&self) -> PluginResult<&client::BitbucketClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }

    async fn fetch_all_repositories(&self) -> PluginResult<Vec<types::Repository>> {
        let client = self.client()?;
        let mut all_repos = Vec::new();
        let mut page = 1;

        const MAX_PAGES: usize = 50;

        loop {
            let params = PaginationParams {
                page,
                page_size: 100,
            };
            let response = client.list_all_repositories(&params).await?;

            if response.items.is_empty() {
                break;
            }

            all_repos.extend(response.items);

            if !response.has_more || page >= MAX_PAGES {
                break;
            }
            page += 1;
        }

        if let Some(selected) = config::parse_selected_items(&self.config) {
            Ok(all_repos
                .into_iter()
                .filter(|r| selected.contains(&r.full_name))
                .collect())
        } else {
            Ok(all_repos)
        }
    }
}

fn collect_partial<T>(results: Vec<PluginResult<T>>, item: &str) -> PluginResult<Vec<T>> {
    let mut kept = Vec::with_capacity(results.len());
    for result in results {
        match result {
            Ok(value) => kept.push(value),
            Err(e) if e.is_transient() => return Err(e),
            Err(e) => tracing::warn!(error = %e, item, "Skipping failed Bitbucket item"),
        }
    }
    Ok(kept)
}

#[async_trait]
impl Plugin for BitbucketPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn provider_type(&self) -> &str {
        "bitbucket"
    }

    fn status_source(&self) -> Option<&StatusSource> {
        self.status_source.as_ref()
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let (email, api_token) = config::get_auth(&config)?;
        let api_url = config::get_api_url();

        let credentials = format!("{}:{}", email, api_token);
        let encoded = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            credentials.as_bytes(),
        );
        let auth_value = format!("Basic {}", encoded);

        let client = http_client.unwrap_or_else(|| {
            std::sync::Arc::new(
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

        self.status_source = Some(config::status_source()?);
        self.client = Some(client::BitbucketClient::new(client, api_url, auth_value));
        self.provider_id = Some(provider_id);
        self.config = config;

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client()?;
        client.get_user().await?;
        Ok(true)
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        let client = self.client()?;
        let workspaces = client.list_workspaces().await?;

        Ok(workspaces
            .into_iter()
            .map(|w| Organization {
                id: w.uuid,
                name: w.name,
                description: Some(w.slug),
            })
            .collect())
    }

    async fn ids_with_ci(&self, ids: Vec<String>) -> PluginResult<Vec<String>> {
        let client = self.client()?;
        let repos: HashMap<String, (String, String)> = self
            .fetch_all_repositories()
            .await?
            .into_iter()
            .map(|repo| (repo.full_name, (repo.workspace.slug, repo.slug)))
            .collect();
        Ok(retain_with_ci(ids, |id| {
            let repo = repos.get(&id).cloned();
            async move {
                let Some((workspace, slug)) = repo else {
                    return Ok(true);
                };
                if !client
                    .list_pipelines(&workspace, &slug, 1)
                    .await?
                    .is_empty()
                {
                    return Ok(true);
                }
                client.has_pipelines_file(&workspace, &slug).await
            }
        })
        .await)
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        let params = params.unwrap_or_default();

        let response = client.list_all_repositories(&params).await?;

        let available = response
            .items
            .iter()
            .map(mapper::map_available_pipeline)
            .collect();

        Ok(PaginatedResponse::new(
            available,
            response.page,
            response.page_size,
            response.total_count,
        ))
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let client = self.client()?;
        let params = params.unwrap_or_default();

        let response = if let Some(workspace) = &org {
            client.list_repositories(workspace, &params).await?
        } else {
            client.list_all_repositories(&params).await?
        };

        let mut available: Vec<AvailablePipeline> = response
            .items
            .iter()
            .map(mapper::map_available_pipeline)
            .collect();

        if let Some(search_term) = search {
            let search_lower = search_term.to_lowercase();
            available.retain(|p| {
                p.name.to_lowercase().contains(&search_lower)
                    || p.id.to_lowercase().contains(&search_lower)
            });
        }

        Ok(PaginatedResponse::new(
            available,
            params.page,
            params.page_size,
            response.total_count,
        ))
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self
            .provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))?;

        let client = self.client()?;
        let repos = self.fetch_all_repositories().await?;

        let pipeline_futures = repos.iter().map(|repo| async move {
            let pipelines = client
                .list_pipelines(&repo.workspace.slug, &repo.slug, 1)
                .await?;
            let latest = pipelines.first();
            if latest.is_none()
                && !client
                    .has_pipelines_file(&repo.workspace.slug, &repo.slug)
                    .await
                    .unwrap_or(true)
            {
                return Ok(None);
            }
            Ok(Some(mapper::map_pipeline(repo, latest, provider_id)))
        });

        Ok(
            collect_partial(join_all(pipeline_futures).await, "repository")?
                .into_iter()
                .flatten()
                .collect(),
        )
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let (provider_id, workspace, repo_slug) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;

        let pipelines = client.list_pipelines(&workspace, &repo_slug, limit).await?;

        Ok(pipelines
            .iter()
            .map(|p| mapper::map_pipeline_run(p, &workspace, &repo_slug, provider_id))
            .collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let (provider_id, workspace, repo_slug) = config::parse_pipeline_id(pipeline_id)?;

        let pipeline = self
            .client()?
            .find_pipeline_by_build_number(&workspace, &repo_slug, run_number)
            .await?;

        Ok(mapper::map_pipeline_run(
            &pipeline,
            &workspace,
            &repo_slug,
            provider_id,
        ))
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let (_, workspace, repo_slug) = config::parse_pipeline_id(run.pipeline_id)?;
        let client = self.client()?;

        let pipeline = client
            .find_pipeline_by_build_number(&workspace, &repo_slug, run.run_number)
            .await?;
        let steps = client
            .list_steps(&workspace, &repo_slug, &pipeline.uuid)
            .await?;

        Ok(mapper::steps_to_log_nodes(&steps))
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        let (_, workspace, repo_slug) = config::parse_pipeline_id(run.pipeline_id)?;
        config::check_step_uuid(node_id)?;
        let client = self.client()?;

        let pipeline = client
            .find_pipeline_by_build_number(&workspace, &repo_slug, run.run_number)
            .await?;

        let position = from
            .and_then(|cursor| cursor.parse::<LogPosition>())
            .filter(|position| position.pipeline == pipeline.uuid && position.step == node_id);
        if from.is_some() && position.is_none() {
            sink.restart()?;
        }
        let (mode, offset) = client
            .stream_step_log(
                &workspace,
                &repo_slug,
                &pipeline.uuid,
                node_id,
                position.map(|p| p.offset),
                sink,
            )
            .await?;
        Ok(LogRead {
            mode,
            cursor: Some(LogCursor::new(&LogPosition {
                pipeline: pipeline.uuid,
                step: node_id.to_string(),
                offset,
            })?),
        })
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &BitbucketLogDialect
    }

    async fn fetch_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        Ok(vec![
            WorkflowParameter {
                name: "ref".to_string(),
                label: Some("Branch".to_string()),
                description: Some("Branch name to run pipeline on".to_string()),
                param_type: WorkflowParameterType::String {
                    default: Some("main".to_string()),
                },
                required: true,
            },
            WorkflowParameter {
                name: "custom_pipeline".to_string(),
                label: Some("Custom Pipeline".to_string()),
                description: Some("Name of custom pipeline to run (optional)".to_string()),
                param_type: WorkflowParameterType::String { default: None },
                required: false,
            },
        ])
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let (_, workspace, repo_slug) = config::parse_pipeline_id(&params.workflow_id)?;
        let client = self.client()?;

        let ref_name = params
            .inputs
            .as_ref()
            .and_then(|i| i.get("ref"))
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string();

        let custom_pipeline = params
            .inputs
            .as_ref()
            .and_then(|i| i.get("custom_pipeline"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty());

        let request = types::TriggerPipelineRequest {
            target: types::TriggerTarget {
                target_type: "pipeline_ref_target".to_string(),
                ref_name,
                ref_type: "branch".to_string(),
                selector: custom_pipeline.map(|name| types::TriggerSelector {
                    selector_type: "custom".to_string(),
                    pattern: Some(name.to_string()),
                }),
            },
        };

        let pipeline = client
            .trigger_pipeline(&workspace, &repo_slug, request)
            .await?;

        let url = pipeline.links.html.map(|l| l.href).unwrap_or_else(|| {
            format!(
                "https://bitbucket.org/{}/{}/pipelines/results/{}",
                workspace, repo_slug, pipeline.build_number
            )
        });

        Ok(url)
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let (_, workspace, repo_slug) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;

        let pipeline = client
            .find_pipeline_by_build_number(&workspace, &repo_slug, run_number)
            .await?;

        let state = pipeline.state.name.as_str();
        if state == "PAUSED" || state == "HALTED" {
            return Err(PluginError::ApiError(format!(
                "Cannot cancel pipeline in '{}' state. Pipelines waiting for manual approval \
                 must be cancelled directly in Bitbucket UI.",
                state
            )));
        }

        if state == "COMPLETED" {
            return Err(PluginError::ApiError(
                "Cannot cancel a completed pipeline.".to_string(),
            ));
        }

        if state == "IN_PROGRESS" {
            let pipeline_is_paused = pipeline
                .state
                .stage
                .as_ref()
                .map(|s| s.name == "PAUSED")
                .unwrap_or(false);

            if pipeline_is_paused {
                return Err(PluginError::ApiError(
                    "Cannot cancel pipeline waiting for manual approval. \
                     Please cancel directly in Bitbucket UI."
                        .to_string(),
                ));
            }

            let steps = client
                .list_steps(&workspace, &repo_slug, &pipeline.uuid)
                .await?;

            let has_paused_step = steps.iter().any(|step| {
                step.state.name == "PENDING"
                    && step
                        .state
                        .stage
                        .as_ref()
                        .map(|s| s.name == "PAUSED")
                        .unwrap_or(false)
            });

            if has_paused_step {
                return Err(PluginError::ApiError(
                    "Cannot cancel pipeline with steps waiting for manual approval. \
                     Please cancel directly in Bitbucket UI."
                        .to_string(),
                ));
            }
        }

        client
            .stop_pipeline(&workspace, &repo_slug, &pipeline.uuid)
            .await
    }

    async fn fetch_agents(&self) -> PluginResult<Vec<BuildAgent>> {
        Err(PluginError::NotSupported(
            "Bitbucket Pipelines uses Atlassian infrastructure - no agent monitoring".to_string(),
        ))
    }

    async fn fetch_artifacts(&self, _run_id: &str) -> PluginResult<Vec<BuildArtifact>> {
        Err(PluginError::NotSupported(
            "Artifact fetching not yet implemented for Bitbucket".to_string(),
        ))
    }

    async fn fetch_queues(&self) -> PluginResult<Vec<BuildQueue>> {
        Err(PluginError::NotSupported(
            "Build queues not supported by Bitbucket Pipelines".to_string(),
        ))
    }

    fn get_migrations(&self) -> Vec<String> {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use std::sync::{
        mpsc,
        Arc,
    };

    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };

    use super::*;

    async fn serve_runs(total: i64) -> (BitbucketPlugin, mpsc::Receiver<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (recorded, requests) = mpsc::channel();

        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buffer = [0; 4096];
                let count = stream.read(&mut buffer).await.unwrap();
                let request = String::from_utf8_lossy(&buffer[..count]);
                let line = request.lines().next().unwrap().to_string();
                let target = line.split_whitespace().nth(1).unwrap();
                let url = reqwest::Url::parse(&format!("http://{address}{target}")).unwrap();
                let (status, body) = match (line.starts_with("GET "), url.path()) {
                    (true, "/repositories/ws/repo/pipelines") => {
                        let page = url
                            .query_pairs()
                            .find(|(key, _)| key == "page")
                            .map(|(_, value)| value.parse::<i64>().unwrap())
                            .unwrap_or(1);
                        let high = total - (page - 1) * 100;
                        let low = (high - 99).max(1);
                        let values: Vec<_> = (low..=high)
                            .rev()
                            .map(|number| {
                                serde_json::json!({
                                    "uuid": format!("{{{number}}}"),
                                    "build_number": number,
                                    "state": if number == 1 {
                                        serde_json::json!({"name": "IN_PROGRESS"})
                                    } else {
                                        serde_json::json!({
                                            "name": "COMPLETED",
                                            "result": {"name": "SUCCESSFUL"}
                                        })
                                    },
                                    "target": {"type": "pipeline_ref_target", "ref_name": "main"},
                                    "created_on": "2026-10-01T00:00:00Z",
                                    "links": {}
                                })
                            })
                            .collect();
                        let next = (low > 1).then(|| format!(
                            "http://{address}/repositories/ws/repo/pipelines?pagelen=100&sort=-created_on&page={}",
                            page + 1
                        ));
                        (
                            "200 OK",
                            serde_json::json!({
                                "values": values, "next": next, "pagelen": 100
                            })
                            .to_string(),
                        )
                    }
                    (true, "/repositories") => {
                        let repo = |slug: &str| {
                            serde_json::json!({
                                "uuid": format!("{{{slug}}}"), "name": slug,
                                "full_name": format!("ws/{slug}"), "slug": slug,
                                "workspace": {"uuid": "{ws}", "slug": "ws", "name": "ws"},
                                "links": {"html": {"href": format!("http://{address}/ws/{slug}")}}
                            })
                        };
                        (
                            "200 OK",
                            serde_json::json!({
                                "values": [repo("repo"), repo("empty"), repo("configured")],
                                "size": 3, "pagelen": 100
                            })
                            .to_string(),
                        )
                    }
                    (true, "/repositories/ws/empty/pipelines")
                    | (true, "/repositories/ws/configured/pipelines") => (
                        "200 OK",
                        serde_json::json!({"values": [], "pagelen": 100}).to_string(),
                    ),
                    (true, "/repositories/ws/configured/src/HEAD/bitbucket-pipelines.yml") => {
                        ("200 OK", "pipelines: {}".to_string())
                    }
                    (true, "/repositories/ws/repo/pipelines/%7B1%7D/steps") => (
                        "200 OK",
                        serde_json::json!({"values": [], "pagelen": 100}).to_string(),
                    ),
                    (false, "/repositories/ws/repo/pipelines/%7B1%7D/stopPipeline") => {
                        ("204 No Content", String::new())
                    }
                    _ => ("404 Not Found", "{}".to_string()),
                };
                recorded.send(line).unwrap();
                let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });

        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut plugin = BitbucketPlugin::new();
        plugin.client = Some(client::BitbucketClient::new(
            Arc::new(reqwest::Client::builder().no_proxy().build().unwrap()),
            format!("http://{address}"),
            "Basic stub".to_string(),
        ));
        plugin.provider_id = Some(1);
        (plugin, requests)
    }

    #[tokio::test]
    async fn run_details_finds_old_builds_without_a_page_cap() {
        let (plugin, requests) = serve_runs(1150).await;

        let run = plugin
            .fetch_run_details("bitbucket__1__ws__repo", 1)
            .await
            .unwrap();
        assert_eq!(run.run_number, 1);
        assert_eq!(run.status, PipelineStatus::Running);
        assert_eq!(requests.try_iter().count(), 12);

        let latest = plugin
            .fetch_run_details("bitbucket__1__ws__repo", 1150)
            .await
            .unwrap();
        assert_eq!(latest.run_number, 1150);
        assert_eq!(latest.status, PipelineStatus::Success);
        assert_eq!(requests.try_iter().count(), 1);
    }

    #[tokio::test]
    async fn only_repositories_without_runs_and_pipelines_file_are_dropped_from_a_selection() {
        let (plugin, requests) = serve_runs(3).await;

        let kept = plugin
            .ids_with_ci(
                ["ws/repo", "ws/empty", "ws/configured"]
                    .map(String::from)
                    .to_vec(),
            )
            .await
            .unwrap();

        assert_eq!(kept, ["ws/repo", "ws/configured"]);
        assert!(requests
            .try_iter()
            .any(|line| line.contains("/ws/empty/src/HEAD/bitbucket-pipelines.yml")));
    }

    #[tokio::test]
    async fn tracking_every_repository_lists_those_with_runs_or_a_pipelines_file() {
        let (plugin, _requests) = serve_runs(3).await;

        let pipelines = plugin.fetch_pipelines().await.unwrap();

        assert_eq!(
            pipelines
                .iter()
                .map(|pipeline| (pipeline.id.as_str(), pipeline.status))
                .collect::<Vec<_>>(),
            [
                ("bitbucket__1__ws__repo", PipelineStatus::Success),
                ("bitbucket__1__ws__configured", PipelineStatus::Pending)
            ]
        );
    }

    #[tokio::test]
    async fn run_details_rejects_missing_builds_after_the_last_page() {
        for total in [0, 150] {
            let (plugin, requests) = serve_runs(total).await;

            let error = plugin
                .fetch_run_details("bitbucket__1__ws__repo", 9999)
                .await
                .unwrap_err();
            assert!(matches!(error, PluginError::PipelineNotFound(_)));
            assert_eq!(requests.try_iter().count(), if total == 0 { 1 } else { 2 });
        }
    }

    #[tokio::test]
    async fn cancel_run_stops_an_old_running_build_by_its_uuid() {
        let (plugin, requests) = serve_runs(150).await;

        plugin
            .cancel_run("bitbucket__1__ws__repo", 1)
            .await
            .unwrap();

        let requests: Vec<_> = requests.try_iter().collect();
        assert_eq!(requests.len(), 4);
        assert_eq!(
            requests.last().unwrap(),
            "POST /repositories/ws/repo/pipelines/%7B1%7D/stopPipeline HTTP/1.1"
        );
    }

    const PIPELINE_UUID: &str = "{3f9d6e5c-2b1a-4c7d-8e9f-0a1b2c3d4e5f}";
    const STEP_BUILD: &str = "{1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a61}";
    const STEP_TEST: &str = "{2c1f6d8b-1e22-4a7f-8b68-4d2e3f5a6b72}";
    const STEP_DEPLOY: &str = "{3d207e9c-2f33-4b80-9c79-5e3f4a6b7c83}";
    const STEP_RELEASE: &str = "{4e318fad-3044-4c91-8d8a-6f405b7c8d94}";
    const STEP_SMOKE: &str = "{5f4290be-4155-4da2-9e9b-70516c8d9ea5}";
    const STEP_PRUNED: &str = "{60503acf-5266-4eb3-8fac-81627d9eafb6}";
    const STEP_EXPIRED: &str = "{71614bd0-6377-4fc4-90bd-92738eafb0c7}";
    const BUILD_LOG: &str = "+ npm ci\nadded 214 packages\n+ npm run build\nbuilt in 9s\n";
    const TEST_LOG: &str =
        "+ make test\nrunning 3 tests\ntest result: FAILED. 2 passed; 1 failed\n";

    struct Reply {
        status: &'static str,
        headers: Vec<(&'static str, String)>,
        body: String,
    }

    fn reply(status: &'static str, body: &str) -> Reply {
        Reply {
            status,
            headers: Vec::new(),
            body: body.to_string(),
        }
    }

    type Recorded = Arc<tokio::sync::Mutex<Vec<(String, Option<String>)>>>;

    async fn stub(
        handler: impl Fn(&str, &str) -> Reply + Send + 'static,
    ) -> (SocketAddr, Recorded) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let recorded = Recorded::default();
        let journal = recorded.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buffer = [0; 8192];
                let count = stream.read(&mut buffer).await.unwrap();
                let request = String::from_utf8_lossy(&buffer[..count]);
                let mut lines = request.lines();
                let request_line = lines.next().unwrap().to_string();
                let header = |wanted: &str| {
                    request
                        .lines()
                        .filter_map(|line| line.split_once(':'))
                        .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
                        .map(|(_, value)| value.trim().to_string())
                };
                let target = request_line.split_whitespace().nth(1).unwrap();
                let reply = handler(target, &header("host").unwrap());
                journal
                    .lock()
                    .await
                    .push((request_line.clone(), header("authorization")));
                let headers: String = reply
                    .headers
                    .iter()
                    .map(|(name, value)| format!("{name}: {value}\r\n"))
                    .collect();
                let response = format!(
                    "HTTP/1.1 {}\r\n{headers}content-length: {}\r\nconnection: close\r\n\r\n{}",
                    reply.status,
                    reply.body.len(),
                    reply.body
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        (address, recorded)
    }

    fn steps_path() -> String {
        format!(
            "/repositories/ws/repo/pipelines/{}/steps",
            urlencoding::encode(PIPELINE_UUID)
        )
    }

    fn log_path(step_uuid: &str) -> String {
        format!("{}/{}/log", steps_path(), urlencoding::encode(step_uuid))
    }

    fn step(uuid: &str, name: &str, state: serde_json::Value, started: bool) -> serde_json::Value {
        let mut step = serde_json::json!({"uuid": uuid, "name": name, "state": state});
        if started {
            step["started_on"] = "2026-10-28T10:00:00.000Z".into();
        }
        if state["name"] == "COMPLETED" && started {
            step["completed_on"] = "2026-10-28T10:01:15.000Z".into();
        }
        step
    }

    fn completed(result: &str) -> serde_json::Value {
        serde_json::json!({"name": "COMPLETED", "result": {"name": result}})
    }

    fn steps_page(host: &str, second_page: bool) -> String {
        let (values, next) = if second_page {
            (
                vec![
                    step(
                        STEP_DEPLOY,
                        "Deploy",
                        serde_json::json!({"name": "IN_PROGRESS"}),
                        true,
                    ),
                    step(
                        STEP_RELEASE,
                        "Release",
                        serde_json::json!({"name": "PENDING"}),
                        false,
                    ),
                    step(STEP_SMOKE, "Smoke", completed("NOT_RUN"), false),
                ],
                None,
            )
        } else {
            (
                vec![
                    step(STEP_BUILD, "Build", completed("SUCCESSFUL"), true),
                    step(STEP_TEST, "Test", completed("FAILED"), true),
                ],
                Some(format!("http://{host}{}?pagelen=100&page=2", steps_path())),
            )
        };
        serde_json::json!({"values": values, "next": next, "pagelen": 100}).to_string()
    }

    fn pipelines_page() -> String {
        serde_json::json!({
            "values": [{
                "uuid": PIPELINE_UUID,
                "build_number": 7,
                "state": {"name": "IN_PROGRESS"},
                "target": {"type": "pipeline_ref_target", "ref_name": "main"},
                "created_on": "2026-10-28T10:00:00Z",
                "links": {}
            }],
            "pagelen": 100
        })
        .to_string()
    }

    async fn serve_pipeline_logs() -> (BitbucketPlugin, Recorded, Recorded) {
        let (storage, stored) = stub(|target, _| match target {
            "/storage/test.log" => reply("200 OK", TEST_LOG),
            _ => reply("404 Not Found", "{}"),
        })
        .await;
        let (api, requests) = stub(move |target, host| {
            let url = reqwest::Url::parse(&format!("http://{host}{target}")).unwrap();
            let path = url.path();
            if path == "/repositories/ws/repo/pipelines" {
                reply("200 OK", &pipelines_page())
            } else if path == steps_path() {
                let second_page = url.query_pairs().any(|(key, _)| key == "page");
                reply("200 OK", &steps_page(host, second_page))
            } else if path == log_path(STEP_BUILD) {
                reply("200 OK", BUILD_LOG)
            } else if path == log_path(STEP_TEST) {
                Reply {
                    status: "307 Temporary Redirect",
                    headers: vec![("location", format!("http://{storage}/storage/test.log"))],
                    body: String::new(),
                }
            } else if path == log_path(STEP_EXPIRED) {
                reply("410 Gone", "{}")
            } else {
                reply("404 Not Found", "{}")
            }
        })
        .await;
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut plugin = BitbucketPlugin::new();
        plugin.client = Some(client::BitbucketClient::new(
            Arc::new(reqwest::Client::builder().no_proxy().build().unwrap()),
            format!("http://{api}"),
            "Basic stub".to_string(),
        ));
        plugin.provider_id = Some(1);
        (plugin, requests, stored)
    }

    fn run_7(metadata: &HashMap<String, serde_json::Value>) -> RunRef<'_> {
        RunRef {
            pipeline_id: "bitbucket__1__ws__repo",
            run_number: 7,
            run_id: "bitbucket__1__ws__repo__7",
            metadata,
        }
    }

    async fn read_step(plugin: &BitbucketPlugin, node_id: &str) -> PluginResult<Vec<u8>> {
        let metadata = HashMap::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let mut sink = LogSink::new(tx);
        let result = plugin
            .read_log(run_7(&metadata), node_id, None, &mut sink)
            .await;
        drop(sink);
        let mut bytes = Vec::new();
        while let Some(chunk) = rx.recv().await {
            bytes.extend_from_slice(&chunk);
        }
        result.map(|_| bytes)
    }

    #[tokio::test]
    async fn steps_on_two_pages_become_job_nodes() {
        let (plugin, requests, _) = serve_pipeline_logs().await;
        let metadata = HashMap::new();

        let nodes = plugin.fetch_log_nodes(run_7(&metadata)).await.unwrap();

        let summary: Vec<_> = nodes
            .iter()
            .map(|node| (node.id.as_str(), node.name.as_str(), node.status, node.log))
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    STEP_BUILD,
                    "Build",
                    PipelineStatus::Success,
                    Some(LogAvailability::Ready)
                ),
                (
                    STEP_TEST,
                    "Test",
                    PipelineStatus::Failed,
                    Some(LogAvailability::Ready)
                ),
                (
                    STEP_DEPLOY,
                    "Deploy",
                    PipelineStatus::Running,
                    Some(LogAvailability::Partial)
                ),
                (
                    STEP_RELEASE,
                    "Release",
                    PipelineStatus::Pending,
                    Some(LogAvailability::Pending)
                ),
                (STEP_SMOKE, "Smoke", PipelineStatus::Skipped, None),
            ]
        );
        let lines: Vec<String> = requests
            .lock()
            .await
            .iter()
            .map(|(line, _)| line.clone())
            .collect();
        let steps = steps_path();
        assert_eq!(
            &lines[1..],
            [
                format!("GET {steps}?pagelen=100 HTTP/1.1"),
                format!("GET {steps}?pagelen=100&page=2 HTTP/1.1"),
            ]
        );
    }

    #[tokio::test]
    async fn step_log_streams_into_the_sink() {
        let (plugin, requests, _) = serve_pipeline_logs().await;

        let log = read_step(&plugin, STEP_BUILD).await.unwrap();

        assert_eq!(log, BUILD_LOG.as_bytes());
        let requests = requests.lock().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[1],
            (
                format!("GET {} HTTP/1.1", log_path(STEP_BUILD)),
                Some("Basic stub".to_string())
            )
        );
    }

    #[tokio::test]
    async fn step_log_redirect_is_followed_without_the_api_credentials() {
        let (plugin, requests, stored) = serve_pipeline_logs().await;

        let log = read_step(&plugin, STEP_TEST).await.unwrap();

        assert_eq!(log, TEST_LOG.as_bytes());
        let requests = requests.lock().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[1].1.as_deref(), Some("Basic stub"));
        assert_eq!(
            *stored.lock().await,
            [("GET /storage/test.log HTTP/1.1".to_string(), None)]
        );
    }

    #[tokio::test]
    async fn step_log_that_is_gone_is_not_found() {
        let (plugin, _, _) = serve_pipeline_logs().await;

        for step in [STEP_PRUNED, STEP_EXPIRED] {
            let error = read_step(&plugin, step).await.unwrap_err();
            assert!(
                matches!(&error, PluginError::PipelineNotFound(message) if message.contains("no longer available")),
                "{step}: {error:?}"
            );
        }
    }

    #[tokio::test]
    async fn step_ids_outside_the_braced_uuid_shape_are_rejected_without_a_request() {
        let (plugin, requests, stored) = serve_pipeline_logs().await;

        let rejected = [
            "",
            "{}",
            "1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a61",
            "%7B1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a61%7D",
            "{1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a6}",
            "{1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a611}",
            "{1b0e5c7a0d11-4f6e-9a57-3c1d2e4f5a61--}",
            "{1g0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a61}",
            "{../../../pipelines/%7Bx%7D/stopPipeline}",
            "{1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a61}/log",
            "{1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a61}?x=1",
            "{1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a61}\n",
            " {1b0e5c7a-0d11-4f6e-9a57-3c1d2e4f5a61}",
        ];
        for node_id in rejected {
            let error = read_step(&plugin, node_id).await.unwrap_err();
            assert!(
                matches!(error, PluginError::InvalidConfig(_)),
                "{node_id:?}: {error:?}"
            );
        }

        assert!(requests.lock().await.is_empty());
        assert!(stored.lock().await.is_empty());
    }

    struct StepLog {
        body: Vec<u8>,
        honors_range: bool,
        requests: Vec<(Option<String>, Option<String>)>,
    }

    type SharedStepLog = Arc<std::sync::Mutex<StepLog>>;

    async fn serve_ranged_storage(log: SharedStepLog) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buffer = [0; 8192];
                let count = stream.read(&mut buffer).await.unwrap();
                let request = String::from_utf8_lossy(&buffer[..count]);
                let header = |wanted: &str| {
                    request
                        .lines()
                        .filter_map(|line| line.split_once(':'))
                        .find(|(name, _)| name.eq_ignore_ascii_case(wanted))
                        .map(|(_, value)| value.trim().to_string())
                };
                let range = header("range");
                let (status, content_range, payload) = {
                    let mut log = log.lock().unwrap();
                    log.requests.push((range.clone(), header("authorization")));
                    let total = log.body.len();
                    let start = range
                        .as_deref()
                        .and_then(|value| {
                            value
                                .strip_prefix("bytes=")?
                                .strip_suffix('-')?
                                .parse()
                                .ok()
                        })
                        .filter(|_| log.honors_range);
                    match start {
                        Some(start) if start >= total => (
                            "416 Range Not Satisfiable",
                            format!("content-range: bytes */{total}\r\n"),
                            Vec::new(),
                        ),
                        Some(start) => (
                            "206 Partial Content",
                            format!("content-range: bytes {start}-{}/{total}\r\n", total - 1),
                            log.body[start..].to_vec(),
                        ),
                        None => ("200 OK", String::new(), log.body.clone()),
                    }
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\n{content_range}content-length: {}\r\nconnection: close\r\n\r\n",
                    payload.len()
                );
                stream.write_all(head.as_bytes()).await.unwrap();
                stream.write_all(&payload).await.unwrap();
            }
        });
        address
    }

    async fn serve_running_step(
        body: &[u8], honors_range: bool,
    ) -> (BitbucketPlugin, SharedStepLog) {
        let log = SharedStepLog::new(std::sync::Mutex::new(StepLog {
            body: body.to_vec(),
            honors_range,
            requests: Vec::new(),
        }));
        let storage = serve_ranged_storage(log.clone()).await;
        let (api, _) = stub(move |target, _| {
            let path = reqwest::Url::parse(&format!("http://api{target}"))
                .unwrap()
                .path()
                .to_string();
            if path == "/repositories/ws/repo/pipelines" {
                reply("200 OK", &pipelines_page())
            } else if path == log_path(STEP_DEPLOY) {
                Reply {
                    status: "307 Temporary Redirect",
                    headers: vec![("location", format!("http://{storage}/storage/deploy.log"))],
                    body: String::new(),
                }
            } else {
                reply("404 Not Found", "{}")
            }
        })
        .await;
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut plugin = BitbucketPlugin::new();
        plugin.client = Some(client::BitbucketClient::new(
            Arc::new(reqwest::Client::builder().no_proxy().build().unwrap()),
            format!("http://{api}"),
            "Basic stub".to_string(),
        ));
        plugin.provider_id = Some(1);
        (plugin, log)
    }

    fn cursor(step: &str, offset: u64) -> LogCursor {
        LogCursor::new(
            &serde_json::json!({"pipeline": PIPELINE_UUID, "step": step, "offset": offset}),
        )
        .unwrap()
    }

    async fn read_running_step(
        plugin: &BitbucketPlugin, from: Option<&LogCursor>,
    ) -> (PluginResult<LogRead>, Vec<u8>, bool) {
        let metadata = HashMap::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let mut sink = LogSink::new(tx);
        let result = plugin
            .read_log(run_7(&metadata), STEP_DEPLOY, from, &mut sink)
            .await;
        let restarted = sink.restarted();
        drop(sink);
        let mut bytes = Vec::new();
        while let Some(chunk) = rx.recv().await {
            bytes.extend_from_slice(&chunk);
        }
        (result, bytes, restarted)
    }

    fn storage_requests(log: &SharedStepLog) -> Vec<(Option<String>, Option<String>)> {
        log.lock().unwrap().requests.clone()
    }

    #[tokio::test]
    async fn resumed_step_log_appends_only_the_new_bytes_through_the_redirect() {
        let (plugin, log) = serve_running_step(b"a\nb\n", true).await;

        let (first, bytes, restarted) = read_running_step(&plugin, None).await;
        assert_eq!(
            first.unwrap(),
            LogRead::replace(Some(cursor(STEP_DEPLOY, 4)))
        );
        assert_eq!(bytes, b"a\nb\n");
        assert!(!restarted);

        log.lock().unwrap().body.extend_from_slice(b"c\n");
        let (second, bytes, restarted) =
            read_running_step(&plugin, Some(&cursor(STEP_DEPLOY, 4))).await;
        assert_eq!(second.unwrap(), LogRead::append(cursor(STEP_DEPLOY, 6)));
        assert_eq!(bytes, b"c\n");
        assert!(!restarted);

        let (idle, bytes, restarted) =
            read_running_step(&plugin, Some(&cursor(STEP_DEPLOY, 6))).await;
        assert_eq!(idle.unwrap(), LogRead::append(cursor(STEP_DEPLOY, 6)));
        assert!(bytes.is_empty() && !restarted);
        assert_eq!(
            storage_requests(&log),
            [
                (None, None),
                (Some("bytes=4-".to_string()), None),
                (Some("bytes=6-".to_string()), None)
            ]
        );
    }

    #[tokio::test]
    async fn storage_ignoring_range_restarts_with_the_whole_step_log() {
        let (plugin, log) = serve_running_step(b"a\nb\nc\n", false).await;

        let (read, bytes, restarted) =
            read_running_step(&plugin, Some(&cursor(STEP_DEPLOY, 4))).await;

        assert_eq!(
            read.unwrap(),
            LogRead::replace(Some(cursor(STEP_DEPLOY, 6)))
        );
        assert_eq!(bytes, b"a\nb\nc\n");
        assert!(restarted);
        assert_eq!(
            storage_requests(&log),
            [(Some("bytes=4-".to_string()), None)]
        );
    }

    #[tokio::test]
    async fn shrunk_step_log_restarts_with_the_whole_step_log() {
        let (plugin, log) = serve_running_step(b"a\n", true).await;

        let (read, bytes, restarted) =
            read_running_step(&plugin, Some(&cursor(STEP_DEPLOY, 6))).await;

        assert_eq!(
            read.unwrap(),
            LogRead::replace(Some(cursor(STEP_DEPLOY, 2)))
        );
        assert_eq!(bytes, b"a\n");
        assert!(restarted);
        assert_eq!(
            storage_requests(&log),
            [(Some("bytes=6-".to_string()), None), (None, None)]
        );
    }

    #[tokio::test]
    async fn cursors_of_another_step_or_shape_restart_without_a_range_request() {
        let (plugin, log) = serve_running_step(b"a\nb\n", true).await;
        let other_pipeline = LogCursor::new(
            &serde_json::json!({"pipeline": "{other}", "step": STEP_DEPLOY, "offset": 4}),
        )
        .unwrap();

        for stale in [
            LogCursor::new(&"garbage").unwrap(),
            cursor(STEP_TEST, 4),
            other_pipeline,
        ] {
            let (read, bytes, restarted) = read_running_step(&plugin, Some(&stale)).await;
            assert_eq!(
                read.unwrap(),
                LogRead::replace(Some(cursor(STEP_DEPLOY, 4)))
            );
            assert_eq!(bytes, b"a\nb\n");
            assert!(restarted);
        }

        assert_eq!(
            storage_requests(&log),
            [(None, None), (None, None), (None, None)]
        );
    }
}
