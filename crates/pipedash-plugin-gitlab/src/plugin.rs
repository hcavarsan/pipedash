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

pub struct GitLabPlugin {
    metadata: PluginMetadata,
    client: Option<client::GitLabClient>,
    provider_id: Option<i64>,
    config: HashMap<String, String>,
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

#[async_trait]
impl Plugin for GitLabPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn provider_type(&self) -> &str {
        "gitlab"
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let token = config
            .get("token")
            .ok_or_else(|| PluginError::InvalidConfig("Missing GitLab access token".to_string()))?
            .to_string();

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
            let pipelines = client.get_project_pipelines(project.id, 1).await.ok()?;
            let latest_pipeline = pipelines.first();
            Some(mapper::map_pipeline(project, latest_pipeline, provider_id))
        });

        let results: Vec<Option<Pipeline>> = join_all(pipeline_futures).await;
        Ok(results.into_iter().flatten().collect())
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

        let detailed_pipelines = join_all(detailed_pipeline_futures).await;

        Ok(detailed_pipelines
            .into_iter()
            .filter_map(|result| result.ok())
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
                let body = match url.path() {
                    "/api/v4/user" => serde_json::json!({
                        "id": 7, "username": "alice", "name": "Alice"
                    }),
                    "/api/v4/projects/102/pipelines" => serde_json::json!([]),
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
                    path => panic!("Unexpected stub path: {path}"),
                }
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nX-Total: {total}\r\nConnection: close\r\n\r\n{body}",
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
}
