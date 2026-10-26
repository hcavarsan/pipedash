use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;
use pipedash_plugin_api::*;

use crate::{
    client,
    config,
    mapper,
    metadata,
    types,
};

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
            Ok(mapper::map_pipeline(repo, latest, provider_id))
        });

        collect_partial(join_all(pipeline_futures).await, "repository")
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
}
