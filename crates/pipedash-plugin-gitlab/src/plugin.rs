use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;
use pipedash_plugin_api::*;

use crate::log_dialect::GitlabLogDialect;
use crate::{
    client,
    config,
    mapper,
    metadata,
    types,
};

#[derive(serde::Serialize, serde::Deserialize)]
struct LogPosition {
    job: String,
    offset: u64,
}

pub struct GitLabPlugin {
    metadata: PluginMetadata,
    client: Option<client::GitLabClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
    status_source: Option<StatusSource>,
}

impl Default for GitLabPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl GitLabPlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            client: None,
            provider_id: None,
            config: HashMap::new(),
            status_source: None,
        }
    }

    fn client(&self) -> PluginResult<&client::GitLabClient> {
        self.client
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }

    async fn fetch_all_projects(&self) -> PluginResult<Vec<types::Project>> {
        let client = self.client()?;
        let mut all_projects = Vec::new();
        let mut page = 1;

        loop {
            let params = PaginationParams {
                page,
                page_size: 100,
            };
            let response = client.list_projects(&params).await?;
            if response.items.is_empty() {
                break;
            }
            all_projects.extend(response.items);

            if !response.has_more || page > 100 {
                break;
            }
            page += 1;
        }

        if let Some(selected_paths) = config::parse_selected_items(&self.config) {
            Ok(all_projects
                .into_iter()
                .filter(|p| {
                    let normalized = p.name_with_namespace.replace(" ", "");
                    selected_paths.contains(&normalized)
                })
                .collect())
        } else {
            Ok(all_projects)
        }
    }

    async fn fetch_projects_paginated(
        &self, params: &PaginationParams,
    ) -> PluginResult<PaginatedResponse<types::Project>> {
        let client = self.client()?;
        let response = client.list_projects(params).await?;

        if let Some(selected_paths) = config::parse_selected_items(&self.config) {
            let filtered_items: Vec<_> = response
                .items
                .into_iter()
                .filter(|p| {
                    let normalized = p.name_with_namespace.replace(" ", "");
                    selected_paths.contains(&normalized)
                })
                .collect();
            let filtered_count = filtered_items.len();
            Ok(PaginatedResponse::new(
                filtered_items,
                params.page,
                params.page_size,
                filtered_count,
            ))
        } else {
            Ok(response)
        }
    }
}

fn collect_partial<T>(results: Vec<PluginResult<T>>, item: &str) -> PluginResult<Vec<T>> {
    let mut kept = Vec::with_capacity(results.len());
    for result in results {
        match result {
            Ok(value) => kept.push(value),
            Err(e) if e.is_transient() => return Err(e),
            Err(e) => tracing::warn!(error = %e, item, "Skipping failed GitLab item"),
        }
    }
    Ok(kept)
}

#[async_trait]
impl Plugin for GitLabPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn provider_type(&self) -> &str {
        "gitlab"
    }

    fn status_source(&self) -> Option<&StatusSource> {
        self.status_source.as_ref()
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let token = config
            .get("token")
            .ok_or_else(|| PluginError::InvalidConfig("Missing GitLab access token".to_string()))?
            .to_string();

        let status_source = config::status_source(&config)?;
        let base_url = config::get_base_url(&config);
        let api_url = config::build_api_url(&base_url);

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

        self.status_source = status_source;
        self.client = Some(client::GitLabClient::new(client, api_url, token));
        self.provider_id = Some(provider_id);
        self.config = config;

        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        let client = self.client()?;
        client.get_user().await?;
        Ok(true)
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let params = params.unwrap_or_default();
        let response = self.fetch_projects_paginated(&params).await?;

        let available_pipelines = response
            .items
            .iter()
            .map(mapper::map_available_pipeline)
            .collect();

        Ok(PaginatedResponse::new(
            available_pipelines,
            response.page,
            response.page_size,
            response.total_count,
        ))
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<pipedash_plugin_api::Organization>> {
        let client = self.client()?;
        client.list_groups().await
    }

    async fn ids_with_ci(&self, ids: Vec<String>) -> PluginResult<Vec<String>> {
        let client = self.client()?;
        let projects: HashMap<String, types::Project> = self
            .fetch_all_projects()
            .await?
            .into_iter()
            .map(|project| (project.name_with_namespace.replace(' ', ""), project))
            .collect();
        Ok(retain_with_ci(ids, |id| {
            let project = projects.get(&id).cloned();
            async move {
                let Some(project) = project else {
                    return Ok(true);
                };
                if !client
                    .get_project_pipelines(project.id, 1)
                    .await?
                    .is_empty()
                {
                    return Ok(true);
                }
                client.has_ci_config(&project).await
            }
        })
        .await)
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let params = params.unwrap_or_default();
        let client = self.client()?;
        let response = client.list_projects_filtered(org, search, &params).await?;

        let available_pipelines = response
            .items
            .iter()
            .map(mapper::map_available_pipeline)
            .collect();

        Ok(PaginatedResponse::new(
            available_pipelines,
            response.page,
            response.page_size,
            response.total_count,
        ))
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self
            .provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))?;

        let client = self.client()?;
        let projects = self.fetch_all_projects().await?;

        let pipeline_futures = projects.iter().map(|project| async move {
            let pipelines = client.get_project_pipelines(project.id, 1).await?;
            let latest_pipeline = pipelines.first();
            if latest_pipeline.is_none() && !client.has_ci_config(project).await.unwrap_or(true) {
                return Ok(None);
            }
            Ok(Some(mapper::map_pipeline(
                project,
                latest_pipeline,
                provider_id,
            )))
        });

        Ok(
            collect_partial(join_all(pipeline_futures).await, "project")?
                .into_iter()
                .flatten()
                .collect(),
        )
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let (provider_id, project_id) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;

        let project = client.get_project(project_id).await?;

        let pipeline_list = client.get_project_pipelines(project_id, limit).await?;

        let parts: Vec<&str> = project.name_with_namespace.split('/').collect();
        let namespace = if parts.len() >= 2 {
            Some(parts[..parts.len() - 1].join("/"))
        } else {
            None
        };

        let detailed_pipeline_futures = pipeline_list
            .iter()
            .map(|p| async move { client.get_pipeline(project_id, p.id).await });

        let detailed_pipelines =
            collect_partial(join_all(detailed_pipeline_futures).await, "pipeline run")?;

        Ok(detailed_pipelines
            .into_iter()
            .map(|p| mapper::map_pipeline_run(&p, project_id, provider_id, namespace.as_deref()))
            .collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let (provider_id, project_id) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;

        let project = client.get_project(project_id).await?;
        let pipeline = client.get_pipeline(project_id, run_number).await?;

        let parts: Vec<&str> = project.name_with_namespace.split('/').collect();
        let namespace = if parts.len() >= 2 {
            Some(parts[..parts.len() - 1].join("/"))
        } else {
            None
        };

        Ok(mapper::map_pipeline_run(
            &pipeline,
            project_id,
            provider_id,
            namespace.as_deref(),
        ))
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let (_, project_id) = config::parse_pipeline_id(run.pipeline_id)?;
        let jobs = self
            .client()?
            .get_pipeline_jobs(project_id, run.run_number)
            .await?;

        Ok(mapper::jobs_to_log_nodes(&jobs))
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        if node_id.is_empty() || !node_id.bytes().all(|b| b.is_ascii_digit()) {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid GitLab job ID: {node_id}"
            )));
        }
        let (_, project_id) = config::parse_pipeline_id(run.pipeline_id)?;

        let position = from
            .and_then(|cursor| cursor.parse::<LogPosition>())
            .filter(|position| position.job == node_id);
        if from.is_some() && position.is_none() {
            sink.restart()?;
        }
        let (mode, offset) = self
            .client()?
            .stream_job_log(project_id, node_id, position.map(|p| p.offset), sink)
            .await?;
        let job = node_id.to_string();
        Ok(LogRead {
            mode,
            cursor: Some(LogCursor::new(&LogPosition { job, offset })?),
        })
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &GitlabLogDialect
    }

    async fn fetch_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        Ok(vec![WorkflowParameter {
            name: "ref".to_string(),
            label: Some("Ref".to_string()),
            description: Some("Branch, tag, or commit SHA to run pipeline on".to_string()),
            param_type: WorkflowParameterType::String {
                default: Some("main".to_string()),
            },
            required: true,
        }])
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        let (_provider_id, project_id) = config::parse_pipeline_id(&params.workflow_id)?;

        let client = self.client()?;

        let ref_name = params
            .inputs
            .as_ref()
            .and_then(|inputs| inputs.get("ref"))
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string();

        let variables = params.inputs.as_ref().and_then(|inputs| {
            inputs.get("variables").and_then(|vars| {
                vars.as_object().map(|obj| {
                    obj.iter()
                        .map(|(key, value)| types::PipelineVariable {
                            key: key.clone(),
                            value: value.as_str().unwrap_or("").to_string(),
                            variable_type: Some("env_var".to_string()),
                        })
                        .collect()
                })
            })
        });

        let pipeline = client
            .trigger_pipeline(project_id, ref_name, variables)
            .await?;
        Ok(pipeline.web_url)
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        let (_, project_id) = config::parse_pipeline_id(pipeline_id)?;
        let client = self.client()?;
        client.cancel_pipeline(project_id, run_number).await?;
        Ok(())
    }

    async fn fetch_agents(&self) -> PluginResult<Vec<BuildAgent>> {
        Err(PluginError::NotSupported(
            "GitLab runners monitoring not implemented".to_string(),
        ))
    }

    async fn fetch_artifacts(&self, _run_id: &str) -> PluginResult<Vec<BuildArtifact>> {
        Err(PluginError::NotSupported(
            "Artifacts download not implemented".to_string(),
        ))
    }

    async fn fetch_queues(&self) -> PluginResult<Vec<BuildQueue>> {
        Err(PluginError::NotSupported(
            "Build queues not supported by GitLab".to_string(),
        ))
    }

    fn get_migrations(&self) -> Vec<String> {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{
        AsyncReadExt,
        AsyncWriteExt,
    };

    use super::*;

    fn stub_pipeline(id: i64) -> serde_json::Value {
        serde_json::json!({
            "id": id, "project_id": 200, "status": "success", "ref": "main", "sha": "abc",
            "web_url": format!("https://gitlab.example/team/app/-/pipelines/{id}"),
            "created_at": "2026-10-01T00:00:00Z", "updated_at": "2026-10-01T00:00:00Z",
            "started_at": null, "finished_at": null, "duration": null, "user": null
        })
    }

    async fn stub_gitlab() -> (GitLabPlugin, tokio::task::JoinHandle<()>) {
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
                let target = request.split_whitespace().nth(1).unwrap();
                let url = reqwest::Url::parse(&format!("http://{addr}{target}")).unwrap();
                let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
                let mut total = 0;
                let mut next_page = String::new();
                let mut status = "200 OK";
                let body = match url.path() {
                    "/api/v4/user" => serde_json::json!({
                        "id": 7, "username": "alice", "name": "Alice"
                    }),
                    "/api/v4/projects/102/pipelines" => serde_json::json!([{
                        "id": 9, "status": "success", "ref": "main", "sha": "abc",
                        "web_url": format!("http://{addr}/team/backend/api/-/pipelines/9"),
                        "created_at": "2026-10-26T12:00:00Z", "updated_at": "2026-10-26T12:05:00Z"
                    }]),
                    "/api/v4/projects/101/pipelines"
                    | "/api/v4/projects/103/pipelines"
                    | "/api/v4/projects/104/pipelines" => {
                        serde_json::json!([])
                    }
                    "/api/v4/projects/104/repository/files/.gitlab-ci.yml" => {
                        serde_json::json!({"file_name": ".gitlab-ci.yml"})
                    }
                    "/api/v4/projects/101/repository/files/.gitlab-ci.yml"
                    | "/api/v4/projects/103/repository/files/.gitlab-ci.yml" => {
                        status = "404 Not Found";
                        serde_json::json!({"message": "404 File Not Found"})
                    }
                    "/api/v4/projects/200" => serde_json::json!({
                        "id": 200, "name": "app", "name_with_namespace": "team / app",
                        "description": null, "web_url": format!("http://{addr}/team/app")
                    }),
                    "/api/v4/projects/200/pipelines" => {
                        total = 150_usize;
                        let page = query.get("page").map_or(1, |p| p.parse::<usize>().unwrap());
                        let page_size = query
                            .get("per_page")
                            .map_or(20, |p| p.parse::<usize>().unwrap())
                            .min(100);
                        if page < total.div_ceil(page_size) {
                            next_page = (page + 1).to_string();
                        }
                        serde_json::Value::Array(
                            (1..=150)
                                .rev()
                                .skip((page - 1) * page_size)
                                .take(page_size)
                                .map(stub_pipeline)
                                .collect(),
                        )
                    }
                    path if path.starts_with("/api/v4/projects/200/pipelines/") => {
                        stub_pipeline(path.rsplit('/').next().unwrap().parse().unwrap())
                    }
                    "/api/v4/groups/42/projects"
                    | "/api/v4/projects"
                    | "/api/v4/users/7/projects" => {
                        let recursive = url.path() == "/api/v4/projects"
                            || query
                                .get("include_subgroups")
                                .is_some_and(|value| value == "true");
                        let projects: Vec<_> = [
                            (101, "team / root"),
                            (102, "team / backend / api"),
                            (103, "team / frontend / api"),
                            (104, "team / backend / nested / worker"),
                        ]
                        .into_iter()
                        .filter(|(_, path)| recursive || *path == "team / root")
                        .filter(|(_, path)| {
                            query
                                .get("search")
                                .is_none_or(|search| path.contains(search))
                        })
                        .map(|(id, path)| {
                            serde_json::json!({
                                "id": id,
                                "name": path.rsplit(" / ").next().unwrap(),
                                "name_with_namespace": path,
                                "web_url": format!("http://{addr}/{}", path.replace(" / ", "/"))
                            })
                        })
                        .collect();
                        total = projects.len();
                        let page = query.get("page").unwrap().parse::<usize>().unwrap();
                        let page_size = query.get("per_page").unwrap().parse::<usize>().unwrap();
                        serde_json::Value::Array(
                            projects
                                .into_iter()
                                .skip((page - 1) * page_size)
                                .take(page_size)
                                .collect(),
                        )
                    }
                    "/api/v4/groups" => {
                        let groups: Vec<_> = (1..=105)
                            .map(|id| {
                                let (name, full_path) = match id {
                                    1 => ("backend".to_string(), "team-a/backend".to_string()),
                                    2 => ("backend".to_string(), "team-b/backend".to_string()),
                                    _ => (format!("group-{id}"), format!("group-{id}")),
                                };
                                serde_json::json!({"id": id, "name": name, "full_path": full_path})
                            })
                            .collect();
                        total = groups.len();
                        let page = query.get("page").map_or(1, |p| p.parse::<usize>().unwrap());
                        let page_size = query
                            .get("per_page")
                            .map_or(20, |p| p.parse::<usize>().unwrap())
                            .min(100);
                        if page < total.div_ceil(page_size) {
                            next_page = (page + 1).to_string();
                        }
                        serde_json::Value::Array(
                            groups
                                .into_iter()
                                .skip((page - 1) * page_size)
                                .take(page_size)
                                .collect(),
                        )
                    }
                    path => panic!("Unexpected stub path: {path}"),
                }
                .to_string();
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nX-Total: {total}\r\nX-Next-Page: {next_page}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let mut plugin = GitLabPlugin::new();
        plugin
            .initialize(
                1,
                HashMap::from([
                    ("base_url".to_string(), format!("http://{addr}")),
                    ("token".to_string(), "stub-token".to_string()),
                ]),
                None,
            )
            .unwrap();
        (plugin, server)
    }

    #[tokio::test]
    async fn parent_group_discovers_nested_projects_with_distinct_paths() {
        let (plugin, server) = stub_gitlab().await;
        let response = plugin
            .fetch_available_pipelines_filtered(Some("42".to_string()), None, None)
            .await
            .unwrap();

        let paths: Vec<_> = response.items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(
            paths,
            [
                "team/root",
                "team/backend/api",
                "team/frontend/api",
                "team/backend/nested/worker"
            ]
        );
        let displayed_paths: Vec<_> = response
            .items
            .iter()
            .map(|item| {
                format!(
                    "{}/{}",
                    item.organization.as_deref().unwrap(),
                    item.repository.as_deref().unwrap()
                )
                .replace(' ', "")
            })
            .collect();
        assert_eq!(displayed_paths, paths);
        server.abort();
    }

    #[tokio::test]
    async fn subgroup_search_keeps_pagination_and_total_count() {
        let (plugin, server) = stub_gitlab().await;
        for (page, expected, has_more) in [
            (1, vec!["team/backend/api"], true),
            (2, vec!["team/frontend/api"], true),
            (3, vec![], false),
        ] {
            let response = plugin
                .fetch_available_pipelines_filtered(
                    Some("42".to_string()),
                    Some("api".to_string()),
                    Some(PaginationParams { page, page_size: 1 }),
                )
                .await
                .unwrap();

            assert_eq!(
                response
                    .items
                    .iter()
                    .map(|item| item.id.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(response.total_count, 2);
            assert_eq!(response.page, page);
            assert_eq!(response.has_more, has_more);
        }
        server.abort();
    }

    #[tokio::test]
    async fn saved_subgroup_selection_still_matches_during_refresh() {
        let (mut plugin, server) = stub_gitlab().await;
        plugin
            .config
            .insert("selected_items".to_string(), "team/backend/api".to_string());
        let pipelines = plugin.fetch_pipelines().await.unwrap();

        assert_eq!(
            pipelines
                .iter()
                .map(|pipeline| pipeline.id.as_str())
                .collect::<Vec<_>>(),
            ["gitlab__1__102"]
        );
        assert_eq!(pipelines[0].repository, "team/backend/api");
        server.abort();
    }

    #[tokio::test]
    async fn only_projects_without_runs_and_ci_file_are_dropped_from_a_selection() {
        let (mut plugin, server) = stub_gitlab().await;
        let selection = "team/backend/api,team/frontend/api,team/backend/nested/worker,gone/repo";
        plugin
            .config
            .insert("selected_items".to_string(), selection.to_string());

        let kept = plugin
            .ids_with_ci(selection.split(',').map(String::from).collect())
            .await
            .unwrap();
        server.abort();

        assert_eq!(
            kept,
            [
                "team/backend/api",
                "team/backend/nested/worker",
                "gone/repo"
            ]
        );
    }

    #[tokio::test]
    async fn tracking_every_project_lists_those_with_runs_or_a_ci_file() {
        let (plugin, server) = stub_gitlab().await;

        let pipelines = plugin.fetch_pipelines().await.unwrap();
        server.abort();

        assert_eq!(
            pipelines
                .iter()
                .map(|pipeline| (pipeline.id.as_str(), pipeline.status))
                .collect::<Vec<_>>(),
            [
                ("gitlab__1__102", PipelineStatus::Success),
                ("gitlab__1__104", PipelineStatus::Pending)
            ]
        );
    }

    #[tokio::test]
    async fn group_picker_lists_every_group_by_full_path() {
        let (plugin, server) = stub_gitlab().await;
        let orgs = plugin.fetch_organizations().await.unwrap();
        server.abort();

        assert_eq!(orgs.len(), 106);
        assert_eq!(orgs[0].name, "alice");
        assert_eq!(orgs[1].name, "team-a/backend");
        assert_eq!(orgs[2].name, "team-b/backend");
        assert!(orgs.iter().any(|org| org.id == "105"));
    }

    #[tokio::test]
    async fn run_history_reads_past_the_first_100_pipelines() {
        let (plugin, server) = stub_gitlab().await;
        let all = plugin
            .fetch_run_history("gitlab__1__200", 200)
            .await
            .unwrap();
        let some = plugin
            .fetch_run_history("gitlab__1__200", 120)
            .await
            .unwrap();
        let latest = plugin.fetch_run_history("gitlab__1__200", 1).await.unwrap();
        server.abort();

        let numbers = |runs: &[PipelineRun]| runs.iter().map(|r| r.run_number).collect::<Vec<_>>();
        assert_eq!(numbers(&all), (1..=150).rev().collect::<Vec<_>>());
        assert_eq!(numbers(&some), (31..=150).rev().collect::<Vec<_>>());
        assert_eq!(numbers(&latest), vec![150]);
    }

    type Requests = std::sync::Arc<tokio::sync::Mutex<Vec<String>>>;

    const PIPELINE_JOBS: &str = r#"[
        {"id":41,"name":"compile","stage":"build","status":"success","started_at":"2026-10-27T10:05:00Z","finished_at":"2026-10-27T10:06:00Z","web_url":"https://gitlab.example/team/app/-/jobs/41"},
        {"id":40,"name":"unit","stage":"test","status":"running","started_at":"2026-10-27T10:06:10Z","finished_at":null,"web_url":"https://gitlab.example/team/app/-/jobs/40"},
        {"id":14,"name":"docs","stage":"deploy","status":"skipped","started_at":null,"finished_at":null,"web_url":"https://gitlab.example/team/app/-/jobs/14"},
        {"id":13,"name":"ship","stage":"deploy","status":"manual","started_at":null,"finished_at":null,"web_url":"https://gitlab.example/team/app/-/jobs/13"},
        {"id":12,"name":"lint","stage":"test","status":"success","started_at":"2026-10-27T10:01:00Z","finished_at":"2026-10-27T10:02:00Z","web_url":"https://gitlab.example/team/app/-/jobs/12"},
        {"id":11,"name":"unit","stage":"test","status":"failed","started_at":"2026-10-27T10:01:00Z","finished_at":"2026-10-27T10:03:00Z","web_url":"https://gitlab.example/team/app/-/jobs/11"},
        {"id":10,"name":"compile","stage":"build","status":"failed","started_at":"2026-10-27T10:00:00Z","finished_at":"2026-10-27T10:00:30Z","web_url":"https://gitlab.example/team/app/-/jobs/10"}
    ]"#;

    const JOB_TRACE: &[u8] = b"Running with gitlab-runner 17.0.0\n\
        section_start:1700000000:step_script\r\x1b[0K\x1b[36;1mExecuting step_script\x1b[0;m\n\
        $ make test\n\
        section_end:1700000007:step_script\r\x1b[0K\n";

    async fn stub_jobs_and_trace() -> (GitLabPlugin, Requests, tokio::task::JoinHandle<()>) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Requests::default();
        let seen = requests.clone();
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
                seen.lock().await.push(request.clone());
                let target = request.split_whitespace().nth(1).unwrap();
                let url = reqwest::Url::parse(&format!("http://{addr}{target}")).unwrap();
                let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
                let mut next_page = String::new();
                let (status, content_type, body) = match url.path() {
                    "/api/v4/projects/200/pipelines/77/jobs" => {
                        let jobs: Vec<serde_json::Value> =
                            serde_json::from_str(PIPELINE_JOBS).unwrap();
                        let page = query["page"].parse::<usize>().unwrap();
                        let page_size = query["per_page"].parse::<usize>().unwrap().min(2);
                        if page * page_size < jobs.len() {
                            next_page = (page + 1).to_string();
                        }
                        let items: Vec<_> = jobs
                            .into_iter()
                            .skip((page - 1) * page_size)
                            .take(page_size)
                            .collect();
                        (
                            "200 OK",
                            "application/json",
                            serde_json::Value::Array(items).to_string().into_bytes(),
                        )
                    }
                    "/api/v4/projects/200/jobs/41/trace" => {
                        ("200 OK", "text/plain", JOB_TRACE.to_vec())
                    }
                    _ => (
                        "404 Not Found",
                        "application/json",
                        br#"{"message":"404 Not found"}"#.to_vec(),
                    ),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nX-Next-Page: {next_page}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(head.as_bytes()).await.unwrap();
                stream.write_all(&body).await.unwrap();
            }
        });
        let mut plugin = GitLabPlugin::new();
        plugin
            .initialize(
                1,
                HashMap::from([
                    ("base_url".to_string(), format!("http://{addr}")),
                    ("token".to_string(), "stub-token".to_string()),
                ]),
                None,
            )
            .unwrap();
        (plugin, requests, server)
    }

    fn run_77(metadata: &HashMap<String, serde_json::Value>) -> RunRef<'_> {
        RunRef {
            pipeline_id: "gitlab__1__200",
            run_number: 77,
            run_id: "gitlab__1__200_77",
            metadata,
        }
    }

    async fn read_job(plugin: &GitLabPlugin, node_id: &str) -> PluginResult<Vec<u8>> {
        let metadata = HashMap::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let mut sink = LogSink::new(tx);
        let result = plugin
            .read_log(run_77(&metadata), node_id, None, &mut sink)
            .await;
        drop(sink);
        let mut bytes = Vec::new();
        while let Some(chunk) = rx.recv().await {
            bytes.extend_from_slice(&chunk);
        }
        result.map(|_| bytes)
    }

    async fn targets(requests: &Requests) -> Vec<String> {
        requests
            .lock()
            .await
            .iter()
            .map(|request| request.split_whitespace().nth(1).unwrap().to_string())
            .collect()
    }

    #[tokio::test]
    async fn pipeline_jobs_become_stages_holding_their_latest_jobs() {
        let (plugin, requests, server) = stub_jobs_and_trace().await;
        let metadata = HashMap::new();

        let nodes = plugin.fetch_log_nodes(run_77(&metadata)).await.unwrap();
        server.abort();

        let summary: Vec<_> = nodes
            .iter()
            .map(|node| {
                (
                    node.id.as_str(),
                    node.parent_id.as_deref(),
                    node.kind,
                    node.name.as_str(),
                    node.status,
                    node.log,
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (
                    "stage:build",
                    None,
                    LogNodeKind::Stage,
                    "build",
                    PipelineStatus::Success,
                    None
                ),
                (
                    "41",
                    Some("stage:build"),
                    LogNodeKind::Job,
                    "compile",
                    PipelineStatus::Success,
                    Some(LogAvailability::Ready)
                ),
                (
                    "stage:test",
                    None,
                    LogNodeKind::Stage,
                    "test",
                    PipelineStatus::Running,
                    None
                ),
                (
                    "40",
                    Some("stage:test"),
                    LogNodeKind::Job,
                    "unit",
                    PipelineStatus::Running,
                    Some(LogAvailability::Partial)
                ),
                (
                    "12",
                    Some("stage:test"),
                    LogNodeKind::Job,
                    "lint",
                    PipelineStatus::Success,
                    Some(LogAvailability::Ready)
                ),
                (
                    "stage:deploy",
                    None,
                    LogNodeKind::Stage,
                    "deploy",
                    PipelineStatus::Pending,
                    None
                ),
                (
                    "13",
                    Some("stage:deploy"),
                    LogNodeKind::Job,
                    "ship",
                    PipelineStatus::Pending,
                    Some(LogAvailability::Pending)
                ),
                (
                    "14",
                    Some("stage:deploy"),
                    LogNodeKind::Job,
                    "docs",
                    PipelineStatus::Skipped,
                    None
                ),
            ]
        );
        assert_eq!(
            nodes[1].web_url.as_deref(),
            Some("https://gitlab.example/team/app/-/jobs/41")
        );
        assert_eq!(
            targets(&requests).await,
            (1..=4)
                .map(|page| format!(
                    "/api/v4/projects/200/pipelines/77/jobs?per_page=100&include_retried=true&page={page}"
                ))
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn job_trace_streams_the_raw_text() {
        let (plugin, requests, server) = stub_jobs_and_trace().await;

        let trace = read_job(&plugin, "41").await.unwrap();
        server.abort();

        assert_eq!(trace, JOB_TRACE);
        assert_eq!(
            targets(&requests).await,
            ["/api/v4/projects/200/jobs/41/trace"]
        );
        assert!(requests.lock().await[0]
            .to_lowercase()
            .contains("private-token: stub-token"));
    }

    #[tokio::test]
    async fn missing_job_trace_is_not_found() {
        let (plugin, _, server) = stub_jobs_and_trace().await;

        let error = read_job(&plugin, "999").await.unwrap_err();
        server.abort();

        assert!(
            matches!(error, PluginError::PipelineNotFound(_)),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn job_ids_that_are_not_numbers_are_rejected_without_a_request() {
        let (plugin, requests, server) = stub_jobs_and_trace().await;

        for node_id in [
            "",
            "../../jobs/41",
            "41/trace",
            "41?x=1",
            "-41",
            "4 1",
            "stage:test",
            "４１",
        ] {
            let error = read_job(&plugin, node_id).await.unwrap_err();
            assert!(
                matches!(error, PluginError::InvalidConfig(_)),
                "{node_id}: {error:?}"
            );
        }
        server.abort();

        assert!(requests.lock().await.is_empty());
    }

    struct Trace {
        body: Vec<u8>,
        honors_range: bool,
        ranges: Vec<Option<String>>,
    }

    type SharedTrace = std::sync::Arc<std::sync::Mutex<Trace>>;

    async fn stub_trace(
        body: &[u8], honors_range: bool,
    ) -> (GitLabPlugin, SharedTrace, tokio::task::JoinHandle<()>) {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let trace = SharedTrace::new(std::sync::Mutex::new(Trace {
            body: body.to_vec(),
            honors_range,
            ranges: Vec::new(),
        }));
        let state = trace.clone();
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
                let (status, content_range, payload) = {
                    let mut trace = state.lock().unwrap();
                    trace.ranges.push(range.clone());
                    let total = trace.body.len();
                    let start = range
                        .as_deref()
                        .and_then(|value| {
                            value
                                .strip_prefix("bytes=")?
                                .strip_suffix('-')?
                                .parse()
                                .ok()
                        })
                        .filter(|_| trace.honors_range);
                    match start {
                        Some(start) if start >= total => (
                            "416 Range Not Satisfiable",
                            format!("Content-Range: bytes */{total}\r\n"),
                            Vec::new(),
                        ),
                        Some(start) => (
                            "206 Partial Content",
                            format!("Content-Range: bytes {start}-{}/{total}\r\n", total - 1),
                            trace.body[start..].to_vec(),
                        ),
                        None => ("200 OK", String::new(), trace.body.clone()),
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
        let mut plugin = GitLabPlugin::new();
        plugin
            .initialize(
                1,
                HashMap::from([
                    ("base_url".to_string(), format!("http://{addr}")),
                    ("token".to_string(), "stub-token".to_string()),
                ]),
                None,
            )
            .unwrap();
        (plugin, trace, server)
    }

    fn cursor(job: &str, offset: u64) -> LogCursor {
        LogCursor::new(&serde_json::json!({"job": job, "offset": offset})).unwrap()
    }

    async fn read_trace(
        plugin: &GitLabPlugin, from: Option<&LogCursor>,
    ) -> (PluginResult<LogRead>, Vec<u8>, bool) {
        let metadata = HashMap::new();
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let mut sink = LogSink::new(tx);
        let result = plugin
            .read_log(run_77(&metadata), "40", from, &mut sink)
            .await;
        let restarted = sink.restarted();
        drop(sink);
        let mut bytes = Vec::new();
        while let Some(chunk) = rx.recv().await {
            bytes.extend_from_slice(&chunk);
        }
        (result, bytes, restarted)
    }

    fn ranges(trace: &SharedTrace) -> Vec<Option<String>> {
        trace.lock().unwrap().ranges.clone()
    }

    #[tokio::test]
    async fn resumed_trace_appends_only_the_new_bytes() {
        let (plugin, trace, server) = stub_trace(b"a\nb\n", true).await;

        let (first, bytes, restarted) = read_trace(&plugin, None).await;
        assert_eq!(first.unwrap(), LogRead::replace(Some(cursor("40", 4))));
        assert_eq!(bytes, b"a\nb\n");
        assert!(!restarted);

        trace.lock().unwrap().body.extend_from_slice(b"c\n");
        let (second, bytes, restarted) = read_trace(&plugin, Some(&cursor("40", 4))).await;
        assert_eq!(second.unwrap(), LogRead::append(cursor("40", 6)));
        assert_eq!(bytes, b"c\n");
        assert!(!restarted);

        let (idle, bytes, restarted) = read_trace(&plugin, Some(&cursor("40", 6))).await;
        server.abort();
        assert_eq!(idle.unwrap(), LogRead::append(cursor("40", 6)));
        assert!(bytes.is_empty() && !restarted);
        assert_eq!(
            ranges(&trace),
            [None, Some("bytes=4-".into()), Some("bytes=6-".into())]
        );
    }

    #[tokio::test]
    async fn server_ignoring_range_restarts_with_the_whole_trace() {
        let (plugin, trace, server) = stub_trace(b"a\nb\nc\n", false).await;

        let (read, bytes, restarted) = read_trace(&plugin, Some(&cursor("40", 4))).await;
        server.abort();

        assert_eq!(read.unwrap(), LogRead::replace(Some(cursor("40", 6))));
        assert_eq!(bytes, b"a\nb\nc\n");
        assert!(restarted);
        assert_eq!(ranges(&trace), [Some("bytes=4-".into())]);
    }

    #[tokio::test]
    async fn shrunk_trace_restarts_with_the_whole_trace() {
        let (plugin, trace, server) = stub_trace(b"a\n", true).await;

        let (read, bytes, restarted) = read_trace(&plugin, Some(&cursor("40", 6))).await;
        server.abort();

        assert_eq!(read.unwrap(), LogRead::replace(Some(cursor("40", 2))));
        assert_eq!(bytes, b"a\n");
        assert!(restarted);
        assert_eq!(ranges(&trace), [Some("bytes=6-".into()), None]);
    }

    #[tokio::test]
    async fn unusable_cursors_restart_without_a_range_request() {
        let (plugin, trace, server) = stub_trace(b"a\nb\n", true).await;

        for stale in [LogCursor::new(&"garbage").unwrap(), cursor("39", 4)] {
            let (read, bytes, restarted) = read_trace(&plugin, Some(&stale)).await;
            assert_eq!(read.unwrap(), LogRead::replace(Some(cursor("40", 4))));
            assert_eq!(bytes, b"a\nb\n");
            assert!(restarted);
        }
        server.abort();

        assert_eq!(ranges(&trace), [None, None]);
    }
}
