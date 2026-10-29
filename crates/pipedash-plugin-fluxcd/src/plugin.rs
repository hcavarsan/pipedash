use std::collections::{
    BTreeSet,
    HashMap,
    HashSet,
};

use async_trait::async_trait;
use chrono::{
    SecondsFormat,
    Utc,
};
use futures::{
    stream,
    StreamExt,
    TryStreamExt,
};
use pipedash_plugin_api::{
    collect_controller_health,
    custom_status_source,
    AvailablePipeline,
    ComponentHealth,
    LogDialect,
    LogNode,
    LogSink,
    Organization,
    PaginatedResponse,
    PaginationParams,
    PermissionCheck,
    PermissionStatus,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    Plugin,
    PluginError,
    PluginMetadata,
    PluginResult,
    ResourceState,
    RunRef,
    StatusSource,
    TriggerParams,
};
use serde_json::json;
use tokio::sync::OnceCell;

use crate::{
    client::{
        self,
        FluxClient,
        REQUESTED_AT_ANNOTATION,
    },
    config::{
        self,
        Settings,
        Target,
    },
    log_dialect::FluxcdLogDialect,
    logs::{
        self,
        Node,
    },
    mapper,
    metadata::{
        self,
        PERMISSIONS,
    },
    types::{
        FluxResource,
        Kind,
    },
    workloads::{
        Lookup,
        UnhealthyWorkloads,
        WorkloadKind,
    },
};

const CONCURRENCY: usize = 8;

pub struct FluxcdPlugin {
    metadata: PluginMetadata,
    client: OnceCell<FluxClient>,
    provider_id: Option<i64>,
    settings: Option<Settings>,
    status_source: Option<StatusSource>,
    unhealthy: std::sync::Mutex<UnhealthyWorkloads>,
}

impl Default for FluxcdPlugin {
    fn default() -> Self {
        Self::new()
    }
}

fn flux_not_installed() -> PluginError {
    PluginError::InvalidConfig(
        "The Flux Kustomization (kustomize.toolkit.fluxcd.io/v1) and HelmRelease (helm.toolkit.fluxcd.io/v2) APIs are not served by this cluster. Install Flux (https://fluxcd.io/flux/installation/) or select the kubeconfig context where Flux runs.".to_string(),
    )
}

fn missing_pipeline(target: &Target, provider_id: i64) -> Pipeline {
    let message = format!(
        "{target} was not found in this cluster. Update your resource selection or recreate it in Flux."
    );

    Pipeline {
        id: target.pipeline_id(provider_id),
        provider_id,
        provider_type: config::PROVIDER_TYPE.to_string(),
        name: format!("{}/{}", target.kind, target.name),
        status: PipelineStatus::Pending,
        last_run: None,
        last_updated: Utc::now(),
        repository: format!("{}/(missing)", target.namespace),
        branch: None,
        workflow_file: None,
        resource_state: Some(ResourceState::new("Not found", Some(&message))),
        metadata: HashMap::from([
            ("kind".to_string(), json!(target.kind.as_str())),
            ("namespace".to_string(), json!(target.namespace)),
            ("name".to_string(), json!(target.name)),
            ("missing".to_string(), json!(true)),
            ("reason".to_string(), json!("ResourceNotFound")),
            ("history_scope".to_string(), json!("bounded_reconciliation")),
        ]),
    }
}

fn paginate<T>(
    items: Vec<T>, params: Option<PaginationParams>,
) -> PluginResult<PaginatedResponse<T>> {
    let params = params.unwrap_or_default();
    params.validate().map_err(PluginError::InvalidConfig)?;
    let offset = params
        .calculate_offset()
        .map_err(PluginError::InvalidConfig)?;
    let total_count = items.len();
    let items: Vec<T> = items
        .into_iter()
        .skip(offset)
        .take(params.page_size)
        .collect();
    let has_more = offset.saturating_add(items.len()) < total_count;
    let mut response = PaginatedResponse::new(items, params.page, params.page_size, total_count);
    response.has_more = has_more;

    Ok(response)
}

impl FluxcdPlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            client: OnceCell::new(),
            provider_id: None,
            settings: None,
            status_source: None,
            unhealthy: std::sync::Mutex::default(),
        }
    }

    fn settings(&self) -> PluginResult<&Settings> {
        self.settings
            .as_ref()
            .ok_or_else(|| PluginError::Internal("FluxCD plugin is not initialized".to_string()))
    }

    fn provider_id(&self) -> PluginResult<i64> {
        self.provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))
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
        let (provider_id, target) = config::parse_pipeline_id(pipeline_id)?;
        if provider_id != self.provider_id()? {
            return Err(PluginError::InvalidConfig(format!(
                "Pipeline ID '{pipeline_id}' belongs to provider {provider_id}, not provider {}",
                self.provider_id()?
            )));
        }
        self.settings()?.ensure_allowed(&target)?;

        Ok(target)
    }

    async fn list_scopes(
        &self, scopes: Vec<(Kind, Option<String>)>,
    ) -> PluginResult<Vec<FluxResource>> {
        let client = self.client().await?;
        let lists: Vec<Option<Vec<FluxResource>>> = stream::iter(scopes)
            .map(|(kind, namespace)| async move { client.list(kind, namespace.as_deref()).await })
            .buffer_unordered(CONCURRENCY)
            .try_collect()
            .await?;
        if lists.iter().all(Option::is_none) {
            return Err(flux_not_installed());
        }

        let mut resources: Vec<FluxResource> = lists.into_iter().flatten().flatten().collect();
        resources.sort_by(|a, b| {
            (&a.metadata.namespace, &a.metadata.name, a.kind).cmp(&(
                &b.metadata.namespace,
                &b.metadata.name,
                b.kind,
            ))
        });

        Ok(resources)
    }

    async fn discover(&self, namespaces: &[String]) -> PluginResult<Vec<FluxResource>> {
        let scopes: Vec<(Kind, Option<String>)> = if namespaces.is_empty() {
            Kind::ALL.iter().map(|kind| (*kind, None)).collect()
        } else {
            namespaces
                .iter()
                .flat_map(|namespace| {
                    Kind::ALL
                        .iter()
                        .map(move |kind| (*kind, Some(namespace.clone())))
                })
                .collect()
        };

        self.list_scopes(scopes).await
    }

    async fn selected(
        &self, selection: &[Target],
    ) -> PluginResult<(Vec<FluxResource>, Vec<Target>)> {
        let scopes: BTreeSet<(Kind, &str)> = selection
            .iter()
            .map(|target| (target.kind, target.namespace.as_str()))
            .collect();
        let wanted: HashSet<&Target> = selection.iter().collect();
        let found: Vec<FluxResource> = self
            .list_scopes(
                scopes
                    .into_iter()
                    .map(|(kind, namespace)| (kind, Some(namespace.to_string())))
                    .collect(),
            )
            .await?
            .into_iter()
            .filter(|resource| wanted.contains(&mapper::target_of(resource)))
            .collect();

        let present: HashSet<Target> = found.iter().map(mapper::target_of).collect();
        let missing = selection
            .iter()
            .filter(|target| !present.contains(*target))
            .cloned()
            .collect();

        Ok((found, missing))
    }

    async fn monitored(&self) -> PluginResult<(Vec<FluxResource>, Vec<Target>)> {
        let settings = self.settings()?;
        if settings.selection.is_empty() {
            Ok((self.discover(&settings.namespaces).await?, Vec::new()))
        } else {
            self.selected(&settings.selection).await
        }
    }

    async fn available(&self) -> PluginResult<Vec<AvailablePipeline>> {
        let resources = self.discover(&self.settings()?.namespaces).await?;

        Ok(resources
            .iter()
            .map(mapper::map_available_pipeline)
            .collect())
    }

    async fn runs(
        &self, pipeline_id: &str,
    ) -> PluginResult<(Target, FluxResource, Vec<PipelineRun>)> {
        let target = self.target(pipeline_id)?;
        let resource = self.client().await?.get(&target).await?;
        let runs = mapper::build_runs(&resource, self.provider_id()?)?;

        Ok((target, resource, runs))
    }

    async fn run(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<(FluxResource, PipelineRun)> {
        let (target, resource, runs) = self.runs(pipeline_id).await?;
        let mut matching: Vec<PipelineRun> = runs
            .into_iter()
            .filter(|run| run.run_number == run_number)
            .collect();

        match matching.len() {
            0 => Err(PluginError::PipelineNotFound(format!(
                "{target} has no record {run_number}. Flux keeps a bounded status.history, so older records are no longer available"
            ))),
            1 => Ok((resource, matching.remove(0))),
            _ => Err(PluginError::ApiError(format!(
                "{target} has {} records numbered {run_number}; refusing to pick one",
                matching.len()
            ))),
        }
    }

    fn scopes(&self) -> PluginResult<Vec<Option<String>>> {
        let namespaces = self.settings()?.monitored_namespaces();

        Ok(if namespaces.is_empty() {
            vec![None]
        } else {
            namespaces.into_iter().map(Some).collect()
        })
    }

    async fn workload_issues(&self, found: &[FluxResource]) -> HashMap<Target, Vec<String>> {
        let Ok(client) = self.client().await else {
            return HashMap::new();
        };
        let owners: BTreeSet<Kind> = found
            .iter()
            .filter(|resource| mapper::current_status(resource) == PipelineStatus::Success)
            .map(|resource| resource.kind)
            .collect();
        let lookups: Vec<Lookup> = owners
            .into_iter()
            .flat_map(|owner| WorkloadKind::ALL.map(|kind| (owner, kind)))
            .collect();

        let results =
            stream::iter(lookups)
                .map(|lookup| async move {
                    (lookup, client.unhealthy_workloads(lookup.0, lookup.1).await)
                })
                .buffer_unordered(CONCURRENCY)
                .collect::<Vec<_>>()
                .await;
        let mut findings = Vec::new();
        let mut failed = Vec::new();
        for (lookup, result) in results {
            match result {
                Ok(found) => findings.extend(found),
                Err(error) => {
                    tracing::debug!(%error, "Workload health unavailable; keeping the Flux-reported state");
                    failed.push(lookup);
                }
            }
        }
        let lasting = self
            .unhealthy
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .lasting(findings, &failed, std::time::Instant::now());
        let mut issues: HashMap<Target, Vec<String>> = HashMap::new();
        for finding in lasting {
            issues
                .entry(finding.owner)
                .or_default()
                .push(finding.message);
        }
        issues.values_mut().for_each(|found| found.sort());

        issues
    }
}

#[async_trait]
impl Plugin for FluxcdPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn provider_type(&self) -> &str {
        config::PROVIDER_TYPE
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        _http_client: Option<std::sync::Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        let settings = Settings::from_config(&config)?;
        let status_source = custom_status_source(&config)?;
        self.settings = Some(settings);
        self.status_source = status_source;
        self.provider_id = Some(provider_id);
        self.client = OnceCell::new();

        Ok(())
    }

    fn status_source(&self) -> Option<&StatusSource> {
        self.status_source.as_ref()
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
        paginate(self.available().await?, params)
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        let resources = self.discover(&self.settings()?.namespaces).await?;
        let namespaces: BTreeSet<&str> = resources
            .iter()
            .map(|resource| resource.metadata.namespace.as_str())
            .collect();

        Ok(namespaces
            .into_iter()
            .map(|namespace| Organization {
                id: namespace.to_string(),
                name: namespace.to_string(),
                description: Some("Namespace".to_string()),
            })
            .collect())
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

        paginate(available, params)
    }

    async fn fetch_pipelines(&self) -> PluginResult<Vec<Pipeline>> {
        let provider_id = self.provider_id()?;
        let (found, missing) = self.monitored().await?;
        let mut workload_issues = self.workload_issues(&found).await;

        Ok(found
            .iter()
            .map(|resource| {
                let issues = workload_issues
                    .remove(&mapper::target_of(resource))
                    .unwrap_or_default();
                mapper::map_pipeline(resource, provider_id, &issues)
            })
            .chain(
                missing
                    .iter()
                    .map(|target| missing_pipeline(target, provider_id)),
            )
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

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let (_, record) = self.run(run.pipeline_id, run.run_number).await?;

        Ok(logs::log_nodes(&record))
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, sink: &mut LogSink,
    ) -> PluginResult<()> {
        let node = Node::parse(node_id)?;
        let target = self.target(run.pipeline_id)?;
        let mut out = Vec::new();

        match node {
            Node::Reconciliation => {
                let (resource, record) = self.run(run.pipeline_id, run.run_number).await?;
                logs::write_reconciliation(&resource, &record, &mut out);
            }
            Node::Events => {
                logs::write_events(self.client().await?.events(&target).await?, &mut out);
            }
        }

        sink.write(out.into()).await
    }

    fn log_dialect(&self) -> &'static dyn LogDialect {
        &FluxcdLogDialect
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
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

        let status = mapper::current_status(&resource);
        let (reason, message) = mapper::headline(&resource, status);

        Ok(json!({
            "action": "reconcile_requested",
            "kind": target.kind.as_str(),
            "namespace": target.namespace,
            "name": target.name,
            "annotation": REQUESTED_AT_ANNOTATION,
            "requested_at": requested_at,
            "status_before_request": mapper::status_label(status),
            "reason": reason,
            "message": message,
        })
        .to_string())
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
            "context" => client::available_contexts(config::kubeconfig_source(config)).await,
            "namespaces" => {
                let client =
                    FluxClient::connect(config::kubeconfig_source(config), config::context(config))
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

    async fn check_permissions(&self) -> PluginResult<PermissionStatus> {
        let client = self.client().await?;
        let scopes = self.scopes()?;
        let flux_scope = [Some(self.settings()?.flux_namespace.clone())];
        let cluster_scope: [Option<String>; 1] = [None];
        let mut checks = Vec::new();
        for (index, spec) in PERMISSIONS.iter().enumerate() {
            let (spec_scopes, names): (&[Option<String>], Vec<Option<&str>>) =
                if !spec.names.is_empty() {
                    (
                        flux_scope.as_slice(),
                        spec.names.iter().copied().map(Some).collect(),
                    )
                } else if spec.cluster_wide {
                    (cluster_scope.as_slice(), vec![None])
                } else {
                    (scopes.as_slice(), vec![None])
                };
            for resource in spec.resources {
                for verb in spec.verbs {
                    for scope in spec_scopes {
                        for name in &names {
                            checks.push((index, spec, *resource, *verb, scope.as_deref(), *name));
                        }
                    }
                }
            }
        }
        let mut reviews = Vec::with_capacity(checks.len());
        for batch in checks.chunks(CONCURRENCY) {
            let results = futures::future::try_join_all(batch.iter().map(
                |(index, spec, resource, verb, scope, name)| async move {
                    let allowed = client
                        .can_i(spec.group, resource, *scope, verb, *name)
                        .await?;
                    Ok::<_, PluginError>((*index, allowed))
                },
            ))
            .await?;
            reviews.extend(results);
        }

        let permissions: Vec<PermissionCheck> = PERMISSIONS
            .iter()
            .enumerate()
            .map(|(index, spec)| PermissionCheck {
                permission: spec.permission(),
                granted: reviews
                    .iter()
                    .filter(|(reviewed, _)| *reviewed == index)
                    .all(|(_, allowed)| *allowed),
            })
            .collect();
        let all_granted = permissions
            .iter()
            .filter(|check| check.permission.required)
            .all(|check| check.granted);
        let scope = match self.settings()?.monitored_namespaces().as_slice() {
            [] => "all namespaces".to_string(),
            namespaces => format!("namespaces: {}", namespaces.join(", ")),
        };

        Ok(PermissionStatus {
            permissions,
            all_granted,
            checked_at: Utc::now(),
            metadata: HashMap::from([("scope".to_string(), scope)]),
        })
    }

    async fn check_health(&self) -> PluginResult<Vec<ComponentHealth>> {
        let client = self.client().await?;
        let namespace = self.settings()?.flux_namespace.as_str();
        let checks = futures::future::join_all(config::CONTROLLERS.iter().map(|name| async move {
            client
                .deployment(namespace, name)
                .await
                .map(|deployment| deployment.health(name))
        }))
        .await;

        collect_controller_health(checks)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{
            Read,
            Write,
        },
        net::{
            TcpListener,
            TcpStream,
        },
        sync::mpsc,
        time::Duration,
    };

    use pipedash_plugin_api::{
        HealthLevel,
        LineMarker,
        LogAvailability,
        LogNodeKind,
    };
    use serde_json::{
        json,
        Value,
    };

    use super::*;

    type Recorded = (String, String, String);

    struct Stub {
        server: String,
        requests: mpsc::Receiver<Recorded>,
    }

    impl Stub {
        fn recorded(&self) -> Vec<Recorded> {
            self.requests.try_iter().collect()
        }
    }

    fn read_request(stream: &mut TcpStream) -> Recorded {
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            match stream.read(&mut byte) {
                Ok(0) | Err(_) => break,
                Ok(_) => head.push(byte[0]),
            }
        }
        let head = String::from_utf8_lossy(&head).to_string();
        let mut parts = head.split_whitespace();
        let method = parts.next().unwrap_or_default().to_string();
        let target = parts.next().unwrap_or_default().to_string();
        let length = head
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, value)| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let mut body = vec![0u8; length];
        let _ = stream.read_exact(&mut body);

        (method, target, String::from_utf8_lossy(&body).to_string())
    }

    fn reason(code: u16) -> &'static str {
        match code {
            200 => "OK",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            409 => "Conflict",
            429 => "Too Many Requests",
            503 => "Service Unavailable",
            _ => "Internal Server Error",
        }
    }

    fn stub(handler: impl Fn(&str, &str, &str) -> (u16, String) + Send + 'static) -> Stub {
        let (log, requests) = mpsc::channel();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let (method, target, body) = read_request(&mut stream);
                if method.is_empty() {
                    continue;
                }
                let (code, payload) = handler(&method, &target, &body);
                let _ = log.send((method, target, body));
                let response = format!(
                    "HTTP/1.1 {code} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
                    reason(code),
                    payload.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });

        Stub {
            server: format!("http://{addr}"),
            requests,
        }
    }

    fn kubeconfig_config(server: &str, dir: &tempfile::TempDir) -> HashMap<String, String> {
        let kubeconfig = dir.path().join(format!(
            "kubeconfig-{}",
            server.rsplit(':').next().unwrap_or("0")
        ));
        std::fs::write(
            &kubeconfig,
            format!(
                "apiVersion: v1\nkind: Config\nclusters:\n- name: c\n  cluster:\n    server: {server}\ncontexts:\n- name: ctx\n  context:\n    cluster: c\n    user: u\ncurrent-context: ctx\nusers:\n- name: u\n  user:\n    token: t\n"
            ),
        )
        .unwrap();

        HashMap::from([(
            "kubeconfig".to_string(),
            kubeconfig.to_string_lossy().to_string(),
        )])
    }

    fn plugin(server: &str, dir: &tempfile::TempDir, extra: &[(&str, &str)]) -> FluxcdPlugin {
        let mut config = kubeconfig_config(server, dir);
        config.extend(
            extra
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_string())),
        );
        let mut plugin = FluxcdPlugin::new();
        plugin.initialize(1, config, None).unwrap();

        plugin
    }

    fn status_json(code: u16, reason: &str, message: &str) -> String {
        json!({
            "kind": "Status",
            "apiVersion": "v1",
            "status": "Failure",
            "message": message,
            "reason": reason,
            "code": code,
        })
        .to_string()
    }

    fn list(items: Vec<Value>, next: Option<&str>) -> String {
        let metadata = next.map_or_else(|| json!({}), |token| json!({ "continue": token }));

        json!({ "items": items, "metadata": metadata }).to_string()
    }

    fn ready_status() -> Value {
        json!({
            "observedGeneration": 1,
            "conditions": [{
                "type": "Ready",
                "status": "True",
                "reason": "ReconciliationSucceeded",
                "message": "Applied revision: main@sha1:abc",
                "observedGeneration": 1,
                "lastTransitionTime": "2026-10-20T10:00:05Z",
            }],
        })
    }

    fn flux(kind: &str, namespace: &str, name: &str, spec: Value, status: Value) -> Value {
        json!({
            "kind": kind,
            "metadata": {
                "name": name,
                "namespace": namespace,
                "uid": format!("uid-{namespace}-{name}"),
                "generation": 1,
                "creationTimestamp": "2026-10-20T10:00:00Z",
            },
            "spec": spec,
            "status": status,
        })
    }

    fn kustomization(namespace: &str, name: &str) -> Value {
        flux("Kustomization", namespace, name, json!({}), ready_status())
    }

    fn helm_release(namespace: &str, name: &str) -> Value {
        flux("HelmRelease", namespace, name, json!({}), ready_status())
    }

    fn serve(target: &str, kustomizations: &[Value], helm_releases: &[Value]) -> (u16, String) {
        let path = target.split('?').next().unwrap_or_default();
        let segments: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        let (namespace, rest) = match segments.as_slice() {
            ["apis", _, _, "namespaces", namespace, rest @ ..] => (Some(*namespace), rest),
            ["apis", _, _, rest @ ..] => (None, rest),
            _ => return (404, status_json(404, "NotFound", "unknown path")),
        };
        let items = match rest.first() {
            Some(&"kustomizations") => kustomizations,
            Some(&"helmreleases") => helm_releases,
            _ => {
                return (
                    404,
                    status_json(
                        404,
                        "NotFound",
                        "the server could not find the requested resource",
                    ),
                )
            }
        };
        let mut scoped = items.iter().filter(|item| {
            namespace.is_none_or(|namespace| item["metadata"]["namespace"] == namespace)
        });

        match rest.get(1) {
            None => (200, list(scoped.cloned().collect(), None)),
            Some(name) => scoped
                .find(|item| item["metadata"]["name"] == *name)
                .map_or_else(
                    || {
                        (
                            404,
                            status_json(404, "NotFound", &format!("{name} not found")),
                        )
                    },
                    |item| (200, item.to_string()),
                ),
        }
    }

    fn cluster(kustomizations: Vec<Value>, helm_releases: Vec<Value>) -> Stub {
        stub(move |_, target, _| serve(target, &kustomizations, &helm_releases))
    }

    fn ids(items: &[AvailablePipeline]) -> Vec<&str> {
        items.iter().map(|item| item.id.as_str()).collect()
    }

    const APP: &str = "fluxcd__1__Kustomization__ci__app";

    #[tokio::test]
    async fn namespaced_discovery_lists_each_namespace_and_kind_without_cluster_wide_requests() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster(
            vec![
                kustomization("a", "one"),
                kustomization("b", "two"),
                kustomization("c", "three"),
            ],
            vec![helm_release("b", "chart")],
        );
        let plugin = plugin(&stub.server, &dir, &[("namespaces", "a,b")]);

        let available = plugin.fetch_available_pipelines(None).await.unwrap();

        assert_eq!(
            ids(&available.items),
            vec![
                "Kustomization__a__one",
                "HelmRelease__b__chart",
                "Kustomization__b__two"
            ]
        );
        let requests = stub.recorded();
        assert_eq!(requests.len(), 4);
        assert!(requests
            .iter()
            .all(|(_, target, _)| target.contains("/namespaces/")));
    }

    #[tokio::test]
    async fn empty_namespaces_list_each_kind_once_across_the_cluster() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster(
            vec![kustomization("a", "one"), kustomization("b", "two")],
            vec![],
        );
        let plugin = plugin(&stub.server, &dir, &[]);

        let available = plugin.fetch_available_pipelines(None).await.unwrap();

        assert_eq!(available.total_count, 2);
        let mut targets: Vec<String> = stub
            .recorded()
            .into_iter()
            .map(|(_, target, _)| target.split('?').next().unwrap_or_default().to_string())
            .collect();
        targets.sort();
        assert_eq!(
            targets,
            vec![
                "/apis/helm.toolkit.fluxcd.io/v2/helmreleases",
                "/apis/kustomize.toolkit.fluxcd.io/v1/kustomizations",
            ]
        );
    }

    #[tokio::test]
    async fn listing_follows_continue_tokens_until_every_page_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let stub = stub(|_, target, _| {
            if !target.contains("/kustomizations") {
                return (200, list(vec![], None));
            }
            if target.contains("continue=page-2") {
                (200, list(vec![kustomization("ci", "second")], None))
            } else {
                (
                    200,
                    list(vec![kustomization("ci", "first")], Some("page-2")),
                )
            }
        });
        let plugin = plugin(&stub.server, &dir, &[]);

        let available = plugin.fetch_available_pipelines(None).await.unwrap();

        assert_eq!(
            ids(&available.items),
            vec!["Kustomization__ci__first", "Kustomization__ci__second"]
        );
        assert_eq!(
            stub.recorded()
                .iter()
                .filter(|(_, target, _)| target.contains("/kustomizations"))
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn available_pipelines_are_filtered_before_they_are_paginated() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster(
            vec![
                kustomization("ci", "app-a"),
                kustomization("ci", "app-b"),
                kustomization("prod", "app-c"),
            ],
            vec![],
        );
        let plugin = plugin(&stub.server, &dir, &[]);
        let page = |page| PaginationParams { page, page_size: 1 };

        let first = plugin
            .fetch_available_pipelines_filtered(
                Some("ci".into()),
                Some("APP".into()),
                Some(page(1)),
            )
            .await
            .unwrap();
        let second = plugin
            .fetch_available_pipelines_filtered(
                Some("ci".into()),
                Some("APP".into()),
                Some(page(2)),
            )
            .await
            .unwrap();

        assert_eq!(ids(&first.items), vec!["Kustomization__ci__app-a"]);
        assert_eq!(
            (first.total_count, first.total_pages, first.has_more),
            (2, 2, true)
        );
        assert_eq!(ids(&second.items), vec!["Kustomization__ci__app-b"]);
        assert!(!second.has_more);
    }

    #[tokio::test]
    async fn invalid_pagination_is_rejected_instead_of_wrapping() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster(vec![kustomization("ci", "app")], vec![]);
        let plugin = plugin(&stub.server, &dir, &[]);

        let result = plugin
            .fetch_available_pipelines(Some(PaginationParams {
                page: 0,
                page_size: 10,
            }))
            .await;

        assert!(
            matches!(result, Err(PluginError::InvalidConfig(_))),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn organizations_are_the_namespaces_that_contain_flux_resources() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster(
            vec![kustomization("b", "one"), kustomization("a", "two")],
            vec![helm_release("b", "chart")],
        );
        let plugin = plugin(&stub.server, &dir, &[]);

        let organizations = plugin.fetch_organizations().await.unwrap();

        let names: Vec<&str> = organizations.iter().map(|org| org.id.as_str()).collect();
        assert_eq!(names, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn selection_loads_only_selected_resources_with_one_list_per_namespace_and_kind() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster(
            vec![
                kustomization("ci", "app"),
                kustomization("ci", "other"),
                kustomization("ci", "unselected"),
            ],
            vec![helm_release("ci", "app")],
        );
        let plugin = plugin(
            &stub.server,
            &dir,
            &[(
                "selected_items",
                "Kustomization__ci__app,Kustomization__ci__other",
            )],
        );

        let pipelines = plugin.fetch_pipelines().await.unwrap();

        let found: Vec<&str> = pipelines
            .iter()
            .map(|pipeline| pipeline.id.as_str())
            .collect();
        assert_eq!(
            found,
            vec![
                "fluxcd__1__Kustomization__ci__app",
                "fluxcd__1__Kustomization__ci__other"
            ]
        );
        assert_eq!(cluster_requests(&stub.recorded()), 1);
    }

    #[tokio::test]
    async fn deleting_one_selected_resource_keeps_its_healthy_peer_and_marks_the_missing_one() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster(vec![kustomization("ci", "app")], vec![]);
        let plugin = plugin(
            &stub.server,
            &dir,
            &[(
                "selected_items",
                "Kustomization__ci__app,Kustomization__ci__gone",
            )],
        );

        let pipelines = plugin.fetch_pipelines().await.unwrap();

        assert_eq!(pipelines.len(), 2);
        let healthy = &pipelines[0];
        assert_eq!(healthy.id, "fluxcd__1__Kustomization__ci__app");
        assert_eq!(healthy.status, PipelineStatus::Success);
        assert!(!healthy.metadata.contains_key("missing"));
        assert!(healthy.last_run.is_some());

        let missing = &pipelines[1];
        assert_eq!(missing.id, "fluxcd__1__Kustomization__ci__gone");
        assert_eq!(missing.name, "Kustomization/gone");
        assert_eq!(missing.repository, "ci/(missing)");
        assert_eq!(missing.status, PipelineStatus::Pending);
        assert_eq!(missing.last_run, None);
        assert_eq!(missing.branch, None);
        assert_eq!(missing.workflow_file, None);
        assert_eq!(missing.metadata["missing"], json!(true));
        assert_eq!(missing.metadata["reason"], json!("ResourceNotFound"));
        assert_eq!(
            missing.resource_state,
            Some(ResourceState::new(
                "Not found",
                Some("Kustomization ci/gone was not found in this cluster. Update your resource selection or recreate it in Flux.")
            ))
        );
        assert!(!missing.metadata.contains_key("status_label"));
        assert!(!missing.metadata.contains_key("message"));
        assert_eq!(missing.metadata["namespace"], json!("ci"));
        assert_eq!(missing.metadata["name"], json!("gone"));
        assert!(!missing.metadata.contains_key("uid"));
        assert_eq!(cluster_requests(&stub.recorded()), 1);
    }

    #[tokio::test]
    async fn missing_selected_resource_has_no_runs_and_cannot_be_triggered() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster(vec![kustomization("ci", "app")], vec![]);
        let plugin = plugin(
            &stub.server,
            &dir,
            &[(
                "selected_items",
                "Kustomization__ci__app,Kustomization__ci__gone",
            )],
        );
        let gone = "fluxcd__1__Kustomization__ci__gone";

        let history = plugin.fetch_run_history(gone, 10).await.unwrap_err();
        let details = plugin.fetch_run_details(gone, 1).await.unwrap_err();
        let trigger = plugin
            .trigger_pipeline(TriggerParams {
                workflow_id: gone.to_string(),
                inputs: None,
            })
            .await
            .unwrap_err();

        for error in [history, details, trigger] {
            assert!(
                matches!(&error, PluginError::PipelineNotFound(message) if message.contains("Kustomization ci/gone")),
                "{error:?}"
            );
        }
        assert!(stub.recorded().iter().all(|(method, _, _)| method == "GET"));
    }

    #[tokio::test]
    async fn forbidden_listing_of_a_selected_namespace_is_not_reported_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        let stub = stub(|_, _, _| (403, status_json(403, "Forbidden", "cannot list resource")));
        let plugin = plugin(
            &stub.server,
            &dir,
            &[(
                "selected_items",
                "Kustomization__ci__app,Kustomization__ci__gone",
            )],
        );

        let error = plugin.fetch_pipelines().await.unwrap_err();

        assert!(
            matches!(&error, PluginError::PermissionDenied(message) if message.contains("namespace ci") && message.contains("cannot list resource")),
            "{error:?}"
        );
    }

    #[test]
    fn selection_outside_the_configured_namespaces_is_rejected_when_initializing() {
        let config = HashMap::from([
            ("namespaces".to_string(), "ci".to_string()),
            (
                "selected_items".to_string(),
                "Kustomization__prod__app".to_string(),
            ),
        ]);

        let result = FluxcdPlugin::new().initialize(1, config, None);

        assert!(
            matches!(result, Err(PluginError::InvalidConfig(_))),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn run_history_and_details_come_from_native_history_of_one_resource() {
        let dir = tempfile::tempdir().unwrap();
        let history = json!([
            {
                "digest": "sha256:b",
                "firstReconciled": "2026-10-21T10:00:00Z",
                "lastReconciled": "2026-10-21T12:00:45Z",
                "lastReconciledDuration": "45s",
                "lastReconciledStatus": "ReconciliationSucceeded",
                "totalReconciliations": 4,
                "metadata": { "revision": "main@sha1:abc" },
            },
            {
                "digest": "sha256:a",
                "firstReconciled": "2026-10-20T10:00:00Z",
                "lastReconciled": "2026-10-20T10:00:10Z",
                "lastReconciledDuration": "10s",
                "lastReconciledStatus": "HealthCheckFailed",
                "totalReconciliations": 1,
                "metadata": { "revision": "main@sha1:def" },
            },
        ]);
        let mut status = ready_status();
        status["history"] = history;
        let stub = cluster(
            vec![flux("Kustomization", "ci", "app", json!({}), status)],
            vec![],
        );
        let plugin = plugin(&stub.server, &dir, &[]);

        let all = plugin.fetch_run_history(APP, 10).await.unwrap();
        let newest = plugin.fetch_run_history(APP, 1).await.unwrap();
        let details = plugin
            .fetch_run_details(APP, all[1].run_number)
            .await
            .unwrap();
        let missing = plugin.fetch_run_details(APP, 42).await.unwrap_err();

        assert_eq!(all.len(), 2);
        assert_eq!(newest.len(), 1);
        assert_eq!(newest[0].run_number, all[0].run_number);
        assert_eq!(details.metadata["reason"], json!("HealthCheckFailed"));
        assert_eq!(details.duration_seconds, Some(10));
        assert!(
            matches!(&missing, PluginError::PipelineNotFound(message) if message.contains("Kustomization ci/app")),
            "{missing:?}"
        );
        assert!(stub
            .recorded()
            .iter()
            .all(|(method, target, _)| method == "GET"
                && target
                    == "/apis/kustomize.toolkit.fluxcd.io/v1/namespaces/ci/kustomizations/app"));
    }

    #[tokio::test]
    async fn identities_from_other_providers_or_outside_the_selection_never_reach_the_cluster() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster(vec![kustomization("ci", "app")], vec![]);
        let plugin = plugin(
            &stub.server,
            &dir,
            &[("selected_items", "Kustomization__ci__app")],
        );
        let trigger = |id: &str| TriggerParams {
            workflow_id: id.to_string(),
            inputs: None,
        };

        for id in [
            "fluxcd__2__Kustomization__ci__app",
            "tekton__1__Kustomization__ci__app",
            "fluxcd__1__Kustomization__ci__other",
            "fluxcd__1__HelmRelease__ci__app",
            "fluxcd__1__Kustomization__ci__..",
        ] {
            assert!(plugin.fetch_run_history(id, 5).await.is_err(), "{id}");
            assert!(plugin.fetch_run_details(id, 1).await.is_err(), "{id}");
            assert!(plugin.trigger_pipeline(trigger(id)).await.is_err(), "{id}");
        }
        assert!(stub.recorded().is_empty());
    }

    #[tokio::test]
    async fn trigger_sets_the_requested_at_annotation_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let object = kustomization("ci", "app");
        let stub = stub(move |_, _, _| (200, object.to_string()));
        let plugin = plugin(&stub.server, &dir, &[]);

        let response = plugin
            .trigger_pipeline(TriggerParams {
                workflow_id: APP.to_string(),
                inputs: None,
            })
            .await
            .unwrap();

        let response: Value = serde_json::from_str(&response).unwrap();
        let requests = stub.recorded();
        let patch = requests
            .iter()
            .find(|(method, _, _)| method == "PATCH")
            .unwrap();
        let body: Value = serde_json::from_str(&patch.2).unwrap();
        assert_eq!(body.as_object().map(|object| object.len()), Some(1));
        assert_eq!(
            body["metadata"].as_object().map(|object| object.len()),
            Some(1)
        );
        assert_eq!(
            body["metadata"]["annotations"][REQUESTED_AT_ANNOTATION],
            response["requested_at"]
        );
        assert_eq!(response["status_before_request"], json!("Ready"));
        assert_eq!(
            response["message"],
            json!("Applied revision: main@sha1:abc")
        );
    }

    #[tokio::test]
    async fn trigger_refuses_suspended_resources_without_patching_them() {
        let dir = tempfile::tempdir().unwrap();
        let object = flux(
            "Kustomization",
            "ci",
            "app",
            json!({ "suspend": true }),
            json!({}),
        );
        let stub = stub(move |_, _, _| (200, object.to_string()));
        let plugin = plugin(&stub.server, &dir, &[]);

        let error = plugin
            .trigger_pipeline(TriggerParams {
                workflow_id: APP.to_string(),
                inputs: None,
            })
            .await
            .unwrap_err();

        assert!(
            matches!(&error, PluginError::ApiError(message) if message.contains("Kustomization ci/app is suspended")),
            "{error:?}"
        );
        assert!(stub.recorded().iter().all(|(method, _, _)| method == "GET"));
    }

    #[tokio::test]
    async fn forbidden_requests_are_permission_denials_naming_the_resource() {
        let dir = tempfile::tempdir().unwrap();
        let object = kustomization("ci", "app");
        let stub = stub(move |method, _, _| {
            if method == "PATCH" {
                (
                    403,
                    status_json(
                        403,
                        "Forbidden",
                        "kustomizations \"app\" is forbidden: cannot patch",
                    ),
                )
            } else {
                (200, object.to_string())
            }
        });
        let plugin = plugin(&stub.server, &dir, &[]);

        let error = plugin
            .trigger_pipeline(TriggerParams {
                workflow_id: APP.to_string(),
                inputs: None,
            })
            .await
            .unwrap_err();

        assert!(
            matches!(&error, PluginError::PermissionDenied(message) if message.contains("Kustomization ci/app") && message.contains("cannot patch")),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn forbidden_namespace_fails_the_whole_listing_and_names_the_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let stub = stub(|_, target, _| {
            if target.contains("/namespaces/denied/") {
                (403, status_json(403, "Forbidden", "cannot list resource"))
            } else {
                (200, list(vec![], None))
            }
        });
        let plugin = plugin(&stub.server, &dir, &[("namespaces", "ci,denied")]);

        let error = plugin.fetch_pipelines().await.unwrap_err();

        assert!(
            matches!(&error, PluginError::PermissionDenied(message) if message.contains("namespace denied") && message.contains("cannot list resource")),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn missing_and_failing_resources_report_not_found_and_unavailable_with_names() {
        let dir = tempfile::tempdir().unwrap();
        let stub = stub(|_, target, _| {
            if target.ends_with("/app") {
                (
                    404,
                    status_json(404, "NotFound", "kustomizations \"app\" not found"),
                )
            } else {
                (500, status_json(500, "InternalError", "etcd unavailable"))
            }
        });
        let plugin = plugin(&stub.server, &dir, &[]);

        let missing = plugin.fetch_run_history(APP, 5).await.unwrap_err();
        let failing = plugin
            .fetch_run_history("fluxcd__1__HelmRelease__ci__web", 5)
            .await
            .unwrap_err();

        assert!(
            matches!(&missing, PluginError::PipelineNotFound(message) if message.contains("Kustomization ci/app")),
            "{missing:?}"
        );
        assert!(
            matches!(&failing, PluginError::ServiceUnavailable { message, .. } if message.contains("HelmRelease ci/web") && message.contains("etcd unavailable")),
            "{failing:?}"
        );
    }

    #[tokio::test]
    async fn malformed_resource_errors_name_the_resource() {
        let dir = tempfile::tempdir().unwrap();
        let broken = json!({ "metadata": { "name": "bad", "namespace": "ci" }, "status": { "conditions": "nope" } });
        let stub = cluster(vec![broken], vec![]);
        let plugin = plugin(&stub.server, &dir, &[]);

        let error = plugin.fetch_available_pipelines(None).await.unwrap_err();

        assert!(
            matches!(&error, PluginError::SerializationError(message) if message.contains("Kustomization ci/bad")),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn missing_kubeconfig_is_an_invalid_configuration() {
        let mut plugin = FluxcdPlugin::new();
        plugin
            .initialize(
                1,
                HashMap::from([(
                    "kubeconfig".to_string(),
                    "/nonexistent/kubeconfig".to_string(),
                )]),
                None,
            )
            .unwrap();

        let error = plugin.validate_credentials().await.unwrap_err();

        assert!(
            matches!(&error, PluginError::InvalidConfig(message) if message.contains("No kubeconfig file found")),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn validation_requires_a_served_flux_api_and_never_hides_forbidden() {
        let dir = tempfile::tempdir().unwrap();
        let absent = stub(|_, _, _| {
            (
                404,
                status_json(
                    404,
                    "NotFound",
                    "the server could not find the requested resource",
                ),
            )
        });
        let partial = stub(|_, target, _| {
            if target.contains("/kustomizations") {
                (200, list(vec![], None))
            } else {
                (404, status_json(404, "NotFound", "not found"))
            }
        });
        let forbidden = stub(|_, _, _| (403, status_json(403, "Forbidden", "cannot list")));

        let none = plugin(&absent.server, &dir, &[])
            .validate_credentials()
            .await;
        let one = plugin(&partial.server, &dir, &[])
            .validate_credentials()
            .await;
        let denied = plugin(&forbidden.server, &dir, &[])
            .validate_credentials()
            .await;

        assert!(
            matches!(&none, Err(PluginError::InvalidConfig(message)) if message.contains("Install Flux")),
            "{none:?}"
        );
        assert!(matches!(one, Ok(true)));
        assert!(
            matches!(denied, Err(PluginError::PermissionDenied(_))),
            "{denied:?}"
        );
    }

    fn review_stub(allow: impl Fn(&Value) -> bool + Send + 'static) -> Stub {
        stub(move |_, _, body| {
            let review: Value = serde_json::from_str(body).unwrap_or_default();
            let attributes = &review["spec"]["resourceAttributes"];
            (
                200,
                json!({ "status": { "allowed": allow(attributes) } }).to_string(),
            )
        })
    }

    #[tokio::test]
    async fn permissions_come_from_access_reviews_for_every_required_verb() {
        let dir = tempfile::tempdir().unwrap();
        let stub = review_stub(|attributes| {
            attributes["resource"] == "kustomizations" && attributes["verb"] != "patch"
        });
        let plugin = plugin(&stub.server, &dir, &[("namespaces", "ci")]);

        let status = plugin.check_permissions().await.unwrap();

        let granted: Vec<(&str, bool)> = status
            .permissions
            .iter()
            .map(|check| (check.permission.name.as_str(), check.granted))
            .collect();
        assert_eq!(
            granted,
            vec![
                ("Read Kustomizations", true),
                ("Read HelmReleases", false),
                ("Request Kustomization reconciliation", false),
                ("Request HelmRelease reconciliation", false),
                ("Read Flux controller deployments", false),
                ("Read Flux object events", false),
                ("Read managed workloads", false),
            ]
        );
        assert!(!status.all_granted);
        assert_eq!(status.metadata["scope"], "namespaces: ci");
        let reviewed = stub.recorded();
        assert_eq!(reviewed.len(), 2 * 2 + 2 + 3 + 1 + 3);
        assert!(reviewed.iter().all(|(method, _, body)| method == "POST"
            && (body.contains("\"namespace\":\"ci\"")
                || body.contains("\"namespace\":\"flux-system\"")
                || body.contains("\"namespace\":null"))));
        let cluster_wide = reviewed
            .iter()
            .filter(|(_, _, body)| body.contains("\"namespace\":null"))
            .count();
        assert_eq!(cluster_wide, 3);
    }

    #[tokio::test]
    async fn list_only_access_is_not_reported_as_read_access() {
        let dir = tempfile::tempdir().unwrap();
        let stub = review_stub(|attributes| attributes["verb"] == "list");
        let plugin = plugin(&stub.server, &dir, &[]);

        let status = plugin.check_permissions().await.unwrap();

        assert!(status
            .permissions
            .iter()
            .filter(|check| {
                !["Read Flux object events", "Read managed workloads"]
                    .contains(&check.permission.name.as_str())
            })
            .all(|check| !check.granted));
        assert!(!status.all_granted);
        assert_eq!(status.metadata["scope"], "all namespaces");
    }

    #[tokio::test]
    async fn permission_check_fails_when_the_review_cannot_be_created() {
        let dir = tempfile::tempdir().unwrap();
        let stub = stub(|_, _, _| (500, status_json(500, "InternalError", "review failed")));
        let plugin = plugin(&stub.server, &dir, &[]);

        let result = plugin.check_permissions().await;

        assert!(
            matches!(&result, Err(PluginError::ServiceUnavailable { message, .. }) if message.contains("review failed")),
            "{result:?}"
        );
    }

    fn controller_stub(respond: impl Fn(&str) -> (u16, String) + Send + 'static) -> Stub {
        stub(move |_, target, _| respond(target.rsplit('/').next().unwrap_or_default()))
    }

    fn controller(replicas: i64, available: Option<i64>, condition: Option<&str>) -> (u16, String) {
        let mut status = json!({});
        if let Some(available) = available {
            status["availableReplicas"] = json!(available);
        }
        if let Some(condition) = condition {
            status["conditions"] = json!([{ "type": "Available", "status": condition }]);
        }

        (
            200,
            json!({ "kind": "Deployment", "spec": { "replicas": replicas }, "status": status })
                .to_string(),
        )
    }

    fn ready() -> (u16, String) {
        controller(1, Some(1), Some("True"))
    }

    fn outage(name: &str, level: HealthLevel, detail: &str) -> ComponentHealth {
        ComponentHealth {
            name: name.to_string(),
            level,
            detail: Some(detail.to_string()),
        }
    }

    #[tokio::test]
    async fn healthy_controllers_are_left_out_of_the_report() {
        let dir = tempfile::tempdir().unwrap();
        let stub = controller_stub(|_| ready());
        let plugin = plugin(&stub.server, &dir, &[]);

        let health = plugin.check_health().await.unwrap();

        assert!(health.is_empty(), "{health:?}");
        let mut paths: Vec<String> = stub
            .recorded()
            .into_iter()
            .map(|(method, target, _)| format!("{method} {target}"))
            .collect();
        paths.sort();
        assert_eq!(
            paths,
            [
                "GET /apis/apps/v1/namespaces/flux-system/deployments/helm-controller",
                "GET /apis/apps/v1/namespaces/flux-system/deployments/kustomize-controller",
                "GET /apis/apps/v1/namespaces/flux-system/deployments/source-controller",
            ]
        );
    }

    #[tokio::test]
    async fn a_controller_scaled_to_zero_is_a_major_outage() {
        let dir = tempfile::tempdir().unwrap();
        let stub = controller_stub(|name| {
            if name == "kustomize-controller" {
                controller(0, None, Some("False"))
            } else {
                ready()
            }
        });
        let plugin = plugin(&stub.server, &dir, &[]);

        let health = plugin.check_health().await.unwrap();

        assert_eq!(
            health,
            vec![outage(
                "kustomize-controller",
                HealthLevel::MajorOutage,
                "scaled to zero"
            )]
        );
    }

    #[tokio::test]
    async fn partly_available_controllers_are_degraded_even_when_the_available_condition_is_false()
    {
        let dir = tempfile::tempdir().unwrap();
        let stub = controller_stub(|name| match name {
            "source-controller" => controller(3, Some(2), Some("False")),
            "helm-controller" => controller(2, Some(0), Some("False")),
            _ => ready(),
        });
        let plugin = plugin(&stub.server, &dir, &[]);

        let health = plugin.check_health().await.unwrap();

        assert_eq!(
            health,
            vec![
                outage(
                    "source-controller",
                    HealthLevel::Degraded,
                    "2/3 replicas available"
                ),
                outage("helm-controller", HealthLevel::MajorOutage, "not running"),
            ]
        );
    }

    #[tokio::test]
    async fn a_missing_controller_is_left_out_and_a_forbidden_one_denies_the_check() {
        let dir = tempfile::tempdir().unwrap();
        let missing = controller_stub(|name| match name {
            "helm-controller" => (404, status_json(404, "NotFound", "deployment not found")),
            _ => controller(0, None, None),
        });
        let forbidden = controller_stub(|name| match name {
            "source-controller" => (403, status_json(403, "Forbidden", "cannot get deployments")),
            _ => ready(),
        });

        let health = plugin(&missing.server, &dir, &[])
            .check_health()
            .await
            .unwrap();
        let denied = plugin(&forbidden.server, &dir, &[]).check_health().await;

        assert_eq!(
            health,
            vec![
                outage(
                    "source-controller",
                    HealthLevel::MajorOutage,
                    "scaled to zero"
                ),
                outage(
                    "kustomize-controller",
                    HealthLevel::MajorOutage,
                    "scaled to zero"
                ),
            ]
        );
        assert!(matches!(denied, Err(PluginError::PermissionDenied(_))));
    }

    #[tokio::test]
    async fn controllers_are_read_from_the_configured_flux_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let stub = controller_stub(|_| ready());
        let plugin = plugin(&stub.server, &dir, &[("flux_namespace", " gitops ")]);

        plugin.check_health().await.unwrap();

        let recorded = stub.recorded();
        assert_eq!(recorded.len(), 3);
        assert!(recorded
            .iter()
            .all(|(_, target, _)| target.starts_with("/apis/apps/v1/namespaces/gitops/")));
    }

    #[test]
    fn an_invalid_flux_namespace_is_rejected_when_initializing() {
        let mut plugin = FluxcdPlugin::new();
        let config = HashMap::from([("flux_namespace".to_string(), "Not_Valid".to_string())]);

        let result = plugin.initialize(1, config, None);

        assert!(matches!(result, Err(PluginError::InvalidConfig(_))));
    }

    #[tokio::test]
    async fn unavailable_and_throttled_clusters_fail_the_whole_health_check() {
        let dir = tempfile::tempdir().unwrap();
        let unavailable = controller_stub(|name| {
            if name == "helm-controller" {
                (
                    503,
                    status_json(503, "ServiceUnavailable", "apiserver overloaded"),
                )
            } else {
                ready()
            }
        });
        let throttled = controller_stub(|_| {
            (
                429,
                json!({
                    "kind": "Status",
                    "apiVersion": "v1",
                    "status": "Failure",
                    "message": "too many requests",
                    "reason": "TooManyRequests",
                    "details": { "retryAfterSeconds": 7 },
                    "code": 429
                })
                .to_string(),
            )
        });

        let unavailable_result = plugin(&unavailable.server, &dir, &[]).check_health().await;
        let throttled_result = plugin(&throttled.server, &dir, &[]).check_health().await;

        assert!(
            matches!(&unavailable_result, Err(PluginError::ServiceUnavailable { message, retry_after: None }) if message.contains("apiserver overloaded")),
            "{unavailable_result:?}"
        );
        assert!(
            matches!(&throttled_result, Err(PluginError::RateLimited { retry_after: Some(wait), .. }) if *wait == Duration::from_secs(7)),
            "{throttled_result:?}"
        );
        for stub in [&unavailable, &throttled] {
            let mut targets: Vec<String> = stub
                .recorded()
                .into_iter()
                .map(|(_, target, _)| target)
                .collect();
            let sent = targets.len();
            targets.sort();
            targets.dedup();
            assert_eq!(targets.len(), sent, "a request was sent twice");
        }
    }

    #[tokio::test]
    async fn can_i_scopes_deployment_reviews_to_namespace_and_name() {
        let dir = tempfile::tempdir().unwrap();
        let stub = review_stub(|attributes| {
            attributes["group"] == "apps"
                && attributes["resource"] == "deployments"
                && attributes["namespace"] == "flux-system"
                && attributes["name"] == "source-controller"
        });
        let plugin = plugin(&stub.server, &dir, &[]);
        let client = plugin.client().await.unwrap();

        let allowed = client
            .can_i(
                "apps",
                "deployments",
                Some("flux-system"),
                "get",
                Some("source-controller"),
            )
            .await
            .unwrap();
        let other_name = client
            .can_i(
                "apps",
                "deployments",
                Some("flux-system"),
                "get",
                Some("helm-controller"),
            )
            .await
            .unwrap();
        let other_namespace = client
            .can_i(
                "apps",
                "deployments",
                Some("other"),
                "get",
                Some("source-controller"),
            )
            .await
            .unwrap();

        assert!(allowed);
        assert!(!other_name);
        assert!(!other_namespace);
    }

    #[tokio::test]
    async fn controller_health_permission_is_checked_for_each_controller_in_the_flux_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let stub = review_stub(|attributes| {
            attributes["resource"] == "deployments"
                && attributes["name"] != "helm-controller"
                && attributes["namespace"] == "gitops"
        });
        let plugin = plugin(&stub.server, &dir, &[("flux_namespace", "gitops")]);

        let status = plugin.check_permissions().await.unwrap();

        let controllers = status
            .permissions
            .iter()
            .find(|check| check.permission.name == "Read Flux controller deployments")
            .unwrap();
        assert!(!controllers.granted);
        assert!(!controllers.permission.required);
        let mut names: Vec<String> = stub
            .recorded()
            .into_iter()
            .filter_map(|(_, _, body)| {
                let review: Value = serde_json::from_str(&body).ok()?;
                let attributes = &review["spec"]["resourceAttributes"];
                (attributes["resource"] == "deployments" && attributes["verb"] == "get").then(
                    || {
                        format!(
                            "{}/{}/{}",
                            attributes["namespace"].as_str().unwrap_or_default(),
                            attributes["verb"].as_str().unwrap_or_default(),
                            attributes["name"].as_str().unwrap_or_default()
                        )
                    },
                )
            })
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "gitops/get/helm-controller",
                "gitops/get/kustomize-controller",
                "gitops/get/source-controller",
            ]
        );
    }

    #[test]
    fn a_custom_status_page_becomes_the_status_source_and_garbage_is_rejected() {
        let mut plugin = FluxcdPlugin::new();
        assert!(plugin.status_source().is_none());

        plugin
            .initialize(
                1,
                HashMap::from([(
                    "status_page_url".to_string(),
                    "https://status.example.com".to_string(),
                )]),
                None,
            )
            .unwrap();
        assert_eq!(
            plugin
                .status_source()
                .map(|source| source.feed_url.as_str()),
            Some("https://status.example.com/api/v2/summary.json")
        );

        plugin.initialize(1, HashMap::new(), None).unwrap();
        assert!(plugin.status_source().is_none());

        let result = plugin.initialize(
            1,
            HashMap::from([(
                "status_page_url".to_string(),
                "javascript:alert(1)".to_string(),
            )]),
            None,
        );
        assert!(matches!(result, Err(PluginError::InvalidConfig(_))));
    }

    #[tokio::test]
    async fn cancelling_is_not_supported() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster(vec![], vec![]);
        let plugin = plugin(&stub.server, &dir, &[]);

        let result = plugin.cancel_run(APP, 1).await;

        assert!(
            matches!(result, Err(PluginError::NotSupported(_))),
            "{result:?}"
        );
        assert!(!plugin.metadata().capabilities.cancel);
    }

    #[tokio::test]
    async fn field_options_list_contexts_and_degrade_only_when_namespaces_are_forbidden() {
        let dir = tempfile::tempdir().unwrap();
        let namespaces = stub(|_, target, _| {
            if target.starts_with("/api/v1/namespaces") {
                (
                    200,
                    list(
                        vec![
                            json!({ "metadata": { "name": "b" } }),
                            json!({ "metadata": { "name": "a" } }),
                        ],
                        None,
                    ),
                )
            } else {
                (404, status_json(404, "NotFound", "not found"))
            }
        });
        let forbidden =
            stub(|_, _, _| (403, status_json(403, "Forbidden", "cannot list namespaces")));
        let allowed = kubeconfig_config(&namespaces.server, &dir);
        let denied = kubeconfig_config(&forbidden.server, &dir);
        let plugin = FluxcdPlugin::new();

        let contexts = plugin.get_field_options("context", &allowed).await.unwrap();
        let listed = plugin
            .get_field_options("namespaces", &allowed)
            .await
            .unwrap();
        let hidden = plugin
            .get_field_options("namespaces", &denied)
            .await
            .unwrap();

        assert_eq!(contexts, vec!["ctx"]);
        assert_eq!(listed, vec!["a", "b"]);
        assert!(hidden.is_empty());
    }

    #[tokio::test]
    async fn pasted_kubeconfig_content_is_used_instead_of_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let namespaces = stub(|_, _, _| {
            (
                200,
                list(vec![json!({ "metadata": { "name": "a" } })], None),
            )
        });
        let from_file = kubeconfig_config(&namespaces.server, &dir);
        let config = HashMap::from([
            (
                "kubeconfig".to_string(),
                "/nonexistent/kubeconfig".to_string(),
            ),
            (
                "token".to_string(),
                std::fs::read_to_string(&from_file["kubeconfig"]).unwrap(),
            ),
        ]);
        let plugin = FluxcdPlugin::new();

        let contexts = plugin.get_field_options("context", &config).await.unwrap();
        let listed = plugin
            .get_field_options("namespaces", &config)
            .await
            .unwrap();

        assert_eq!(contexts, vec!["ctx"]);
        assert_eq!(listed, vec!["a"]);
    }

    #[tokio::test]
    async fn incomplete_history_does_not_hide_current_resource_status() {
        let dir = tempfile::tempdir().unwrap();
        let mut status = ready_status();
        status["history"] = json!([{ "digest": "sha256:incomplete" }]);
        let stub = cluster(
            vec![flux("Kustomization", "ci", "app", json!({}), status)],
            vec![],
        );
        let plugin = plugin(&stub.server, &dir, &[]);

        let pipelines = plugin.fetch_pipelines().await.unwrap();
        let history = plugin.fetch_run_history(APP, 10).await;

        assert_eq!(pipelines[0].id, APP);
        assert_eq!(
            pipelines[0].status,
            pipedash_plugin_api::PipelineStatus::Success
        );
        assert_eq!(
            pipelines[0].last_run.unwrap().to_rfc3339(),
            "2026-10-20T10:00:05+00:00"
        );
        assert!(matches!(history, Err(PluginError::SerializationError(_))));
    }

    const LOG_APP: &str = "fluxcd__1__Kustomization__ci__web";
    const LOG_HELM: &str = "fluxcd__1__HelmRelease__ci__podinfo";
    const OPEN: LineMarker = LineMarker::SectionStart { open: true };

    fn cluster_with_events(
        kustomizations: Vec<Value>, helm_releases: Vec<Value>, events: Vec<Value>,
    ) -> Stub {
        stub(move |_, target, _| {
            if target.starts_with("/api/v1/namespaces/ci/events") {
                (200, list(events.clone(), None))
            } else {
                serve(target, &kustomizations, &helm_releases)
            }
        })
    }

    fn condition_json(type_: &str, status: &str, reason: &str, message: &str, at: &str) -> Value {
        json!({
            "type": type_,
            "status": status,
            "reason": reason,
            "message": message,
            "observedGeneration": 1,
            "lastTransitionTime": at,
        })
    }

    fn web_kustomization() -> Value {
        let mut failed = json!({
            "digest": "sha256:a",
            "firstReconciled": "2026-10-20T10:00:00Z",
            "lastReconciled": "2026-10-20T10:00:10Z",
            "lastReconciledDuration": "10s",
            "lastReconciledStatus": "HealthCheckFailed",
            "totalReconciliations": 1,
            "metadata": { "revision": "main@sha1:6b7aab8a1f" },
        });
        failed["metadata"]["originRevision"] = json!("main@sha1:0f3d9c2e44");
        let status = json!({
            "observedGeneration": 1,
            "lastAttemptedRevision": "main@sha1:9c1e0b7d55",
            "lastAppliedRevision": "main@sha1:6b7aab8a1f",
            "history": [failed],
            "conditions": [
                condition_json(
                    "Ready",
                    "False",
                    "ArtifactFailed",
                    "failed to fetch artifact\nconnection refused",
                    "2026-10-21T08:00:00Z",
                ),
                condition_json(
                    "Reconciling",
                    "True",
                    "ProgressingWithRetry",
                    "retrying in 5m",
                    "2026-10-21T08:00:01Z",
                ),
            ],
        });
        let spec = json!({
            "interval": "10m",
            "path": "./apps",
            "sourceRef": { "kind": "GitRepository", "name": "flux-system" },
        });

        flux("Kustomization", "ci", "web", spec, status)
    }

    fn podinfo_release() -> Value {
        let status = json!({
            "observedGeneration": 1,
            "history": [{
                "version": 3,
                "action": "upgrade",
                "status": "failed",
                "chartName": "podinfo",
                "chartVersion": "6.7.1",
                "appVersion": "6.7.1",
                "digest": "sha256:c",
                "configDigest": "sha256:d",
                "firstDeployed": "2026-10-19T09:00:00Z",
                "lastDeployed": "2026-10-20T11:00:00Z",
            }],
            "conditions": [condition_json(
                "Ready",
                "True",
                "UpgradeSucceeded",
                "Helm upgrade succeeded",
                "2026-10-21T08:00:00Z",
            )],
        });

        flux("HelmRelease", "ci", "podinfo", json!({}), status)
    }

    fn warning_events() -> Vec<Value> {
        vec![
            json!({
                "type": "Normal",
                "reason": "ReconciliationSucceeded",
                "message": "Reconciliation finished in 45s",
                "eventTime": "2026-10-20T10:05:00.123456Z",
                "reportingComponent": "kustomize-controller",
                "series": { "count": 2, "lastObservedTime": "2026-10-20T10:07:00.000000Z" },
            }),
            json!({
                "type": "Warning",
                "reason": "HealthCheckFailed",
                "message": "health check failed after 30s",
                "count": 3,
                "firstTimestamp": "2026-10-20T10:00:10Z",
                "lastTimestamp": "2026-10-20T10:02:10Z",
                "source": { "component": "kustomize-controller" },
            }),
            json!({
                "type": "Normal",
                "reason": "Progressing",
                "message": "Reconciliation in progress",
                "lastTimestamp": "2026-10-20T10:00:00Z",
            }),
        ]
    }

    async fn read_raw(
        plugin: &FluxcdPlugin, pipeline_id: &str, run_number: i64, node_id: &str,
    ) -> PluginResult<String> {
        let metadata = HashMap::new();
        let run = RunRef {
            pipeline_id,
            run_number,
            run_id: "",
            metadata: &metadata,
        };
        let (tx, mut rx) = tokio::sync::mpsc::channel(64);
        let mut sink = LogSink::new(tx);
        let result = plugin.read_log(run, node_id, &mut sink).await;
        drop(sink);
        let mut bytes = Vec::new();
        while let Some(chunk) = rx.recv().await {
            bytes.extend_from_slice(&chunk);
        }
        result?;

        Ok(String::from_utf8(bytes).unwrap())
    }

    async fn read_lines(
        plugin: &FluxcdPlugin, pipeline_id: &str, run_number: i64, node_id: &str,
    ) -> PluginResult<Vec<(LineMarker, String)>> {
        let raw = read_raw(plugin, pipeline_id, run_number, node_id).await?;

        Ok(raw
            .lines()
            .map(|line| {
                let parsed = FluxcdLogDialect.parse(line.as_bytes());
                (
                    parsed.marker,
                    String::from_utf8_lossy(parsed.body).into_owned(),
                )
            })
            .collect())
    }

    fn line(marker: LineMarker, text: &str) -> (LineMarker, String) {
        (marker, text.to_string())
    }

    async fn run_numbered(plugin: &FluxcdPlugin, pipeline_id: &str, snapshot: bool) -> i64 {
        plugin
            .fetch_run_history(pipeline_id, 10)
            .await
            .unwrap()
            .into_iter()
            .find(|run| run.metadata.contains_key("snapshot") == snapshot)
            .unwrap()
            .run_number
    }

    #[tokio::test]
    async fn log_nodes_list_the_record_and_the_object_events() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster_with_events(vec![web_kustomization()], vec![], vec![]);
        let plugin = plugin(&stub.server, &dir, &[]);
        let metadata = HashMap::new();
        let node_list = |run_number| {
            let plugin = &plugin;
            let metadata = &metadata;
            async move {
                let run = RunRef {
                    pipeline_id: LOG_APP,
                    run_number,
                    run_id: "",
                    metadata,
                };
                plugin.fetch_log_nodes(run).await.unwrap()
            }
        };

        let history = node_list(run_numbered(&plugin, LOG_APP, false).await).await;
        let current = node_list(run_numbered(&plugin, LOG_APP, true).await).await;

        let summary = |nodes: &[pipedash_plugin_api::LogNode]| {
            nodes
                .iter()
                .map(|node| (node.id.clone(), node.kind, node.status, node.log))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            summary(&history),
            [
                (
                    "reconciliation".to_string(),
                    LogNodeKind::Job,
                    PipelineStatus::Failed,
                    Some(LogAvailability::Partial)
                ),
                (
                    "object-events".to_string(),
                    LogNodeKind::Job,
                    PipelineStatus::Success,
                    Some(LogAvailability::Partial)
                ),
            ]
        );
        assert_eq!(history[0].name, "Reconciliation");
        assert_eq!(history[1].name, "Events of the object");
        assert_eq!(
            history[0].started_at.unwrap().to_rfc3339(),
            "2026-10-20T10:00:00+00:00"
        );
        assert_eq!(
            history[0].finished_at.unwrap().to_rfc3339(),
            "2026-10-20T10:00:10+00:00"
        );
        assert_eq!(current[0].status, PipelineStatus::Failed);
        assert_eq!(current[0].log, Some(LogAvailability::Partial));
    }

    #[tokio::test]
    async fn failed_history_record_reads_its_status_revision_digest_and_times() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster_with_events(vec![web_kustomization()], vec![], vec![]);
        let plugin = plugin(&stub.server, &dir, &[]);
        let number = run_numbered(&plugin, LOG_APP, false).await;

        let lines = read_lines(&plugin, LOG_APP, number, "reconciliation")
            .await
            .unwrap();

        assert_eq!(
            lines,
            [
                line(OPEN, "Reconciliation 6b7aab8 of Kustomization ci/web"),
                line(LineMarker::Error, "Reconcile failed: HealthCheckFailed"),
                line(LineMarker::Plain, "Took 10s"),
                line(LineMarker::Plain, "Revision: main@sha1:6b7aab8a1f"),
                line(LineMarker::Plain, "Origin revision: main@sha1:0f3d9c2e44"),
                line(LineMarker::Plain, "Digest: sha256:a"),
                line(
                    LineMarker::Plain,
                    "First reconciled: 2026-10-20T10:00:00+00:00"
                ),
                line(LineMarker::Plain, "Reconciliations: 1"),
            ]
        );
        let raw = read_raw(&plugin, LOG_APP, number, "reconciliation")
            .await
            .unwrap();
        assert!(raw.starts_with("+2026-10-20T10:00:00Z Reconciliation 6b7aab8"));
        assert!(raw.contains("\n!2026-10-20T10:00:10Z Reconcile failed: HealthCheckFailed\n"));
    }

    #[tokio::test]
    async fn current_state_record_reads_the_conditions_with_reason_and_message() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster_with_events(vec![web_kustomization()], vec![], vec![]);
        let plugin = plugin(&stub.server, &dir, &[]);
        let number = run_numbered(&plugin, LOG_APP, true).await;

        let lines = read_lines(&plugin, LOG_APP, number, "reconciliation")
            .await
            .unwrap();

        assert_eq!(
            lines,
            [
                line(OPEN, "Current state of Kustomization ci/web"),
                line(LineMarker::Error, "Not ready"),
                line(
                    LineMarker::Error,
                    "ArtifactFailed: failed to fetch artifact"
                ),
                line(LineMarker::Error, "connection refused"),
                line(LineMarker::Plain, "Revision: main@sha1:9c1e0b7d55"),
                line(LineMarker::Plain, "Applied revision: main@sha1:6b7aab8a1f"),
                line(LineMarker::Plain, "Source: GitRepository/ci/flux-system"),
                line(LineMarker::Plain, "Interval: 10m"),
                line(OPEN, "Conditions"),
                line(
                    LineMarker::Error,
                    "Ready=False ArtifactFailed: failed to fetch artifact"
                ),
                line(LineMarker::Error, "connection refused"),
                line(
                    LineMarker::Plain,
                    "Reconciling=True ProgressingWithRetry: retrying in 5m"
                ),
            ]
        );
    }

    #[tokio::test]
    async fn helm_release_record_reads_the_chart_version_and_release() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster_with_events(vec![], vec![podinfo_release()], vec![]);
        let plugin = plugin(&stub.server, &dir, &[]);
        let number = run_numbered(&plugin, LOG_HELM, false).await;

        let lines = read_lines(&plugin, LOG_HELM, number, "reconciliation")
            .await
            .unwrap();

        assert_eq!(
            lines,
            [
                line(OPEN, "Release #3 of HelmRelease ci/podinfo"),
                line(LineMarker::Error, "Failed"),
                line(LineMarker::Plain, "Chart version: 6.7.1"),
                line(LineMarker::Plain, "Chart: podinfo@6.7.1"),
                line(LineMarker::Plain, "Release version: 3"),
                line(LineMarker::Plain, "App version: 6.7.1"),
                line(LineMarker::Plain, "Action: upgrade"),
                line(LineMarker::Plain, "Digest: sha256:c"),
                line(LineMarker::Plain, "Config digest: sha256:d"),
                line(
                    LineMarker::Plain,
                    "First deployed: 2026-10-19T09:00:00+00:00"
                ),
            ]
        );
    }

    #[tokio::test]
    async fn object_events_come_in_time_order_with_warnings_marked() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster_with_events(vec![], vec![], warning_events());
        let plugin = plugin(&stub.server, &dir, &[]);

        let raw = read_raw(&plugin, LOG_APP, 0, "object-events")
            .await
            .unwrap();

        assert_eq!(
            raw,
            ".2026-10-20T10:00:00Z Normal Progressing: Reconciliation in progress\n\
             ?2026-10-20T10:02:10Z Warning HealthCheckFailed from kustomize-controller: health check failed after 30s (x3)\n\
             .2026-10-20T10:07:00Z Normal ReconciliationSucceeded from kustomize-controller: Reconciliation finished in 45s (x2)\n"
        );
        let requests = stub.recorded();
        assert_eq!(requests.len(), 1);
        let (method, target, _) = &requests[0];
        assert_eq!(method, "GET");
        assert!(
            target.starts_with("/api/v1/namespaces/ci/events?"),
            "{target}"
        );
        assert!(
            target.contains(
                "fieldSelector=involvedObject.kind%3DKustomization%2CinvolvedObject.name%3Dweb%2CinvolvedObject.namespace%3Dci"
            ),
            "{target}"
        );
    }

    #[tokio::test]
    async fn events_follow_continue_tokens_and_say_so_when_there_are_none() {
        let dir = tempfile::tempdir().unwrap();
        let pages = stub(|_, target, _| {
            let first = !target.contains("continue=");
            let events = if first {
                vec![
                    json!({ "type": "Normal", "reason": "First", "message": "one", "lastTimestamp": "2026-10-20T10:00:00Z" }),
                ]
            } else {
                vec![
                    json!({ "type": "Normal", "reason": "Second", "message": "two", "lastTimestamp": "2026-10-20T10:01:00Z" }),
                ]
            };
            (200, list(events, first.then_some("page-2")))
        });
        let empty = cluster_with_events(vec![], vec![], vec![]);

        let paged = read_lines(
            &plugin(&pages.server, &dir, &[]),
            LOG_APP,
            0,
            "object-events",
        )
        .await
        .unwrap();
        let none = read_lines(
            &plugin(&empty.server, &dir, &[]),
            LOG_APP,
            0,
            "object-events",
        )
        .await
        .unwrap();

        assert_eq!(
            paged,
            [
                line(LineMarker::Plain, "Normal First: one"),
                line(LineMarker::Plain, "Normal Second: two"),
            ]
        );
        assert_eq!(
            none,
            [line(
                LineMarker::Plain,
                "No events. Kubernetes keeps events for one hour by default."
            )]
        );
    }

    #[tokio::test]
    async fn bad_node_ids_are_rejected_without_a_request() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster_with_events(vec![web_kustomization()], vec![], warning_events());
        let plugin = plugin(&stub.server, &dir, &[]);

        for node_id in [
            "",
            "events",
            "pods",
            "Reconciliation",
            "reconciliation/x",
            "reconciliation ",
            "object-events?watch=true",
            "../object-events",
        ] {
            let error = read_raw(&plugin, LOG_APP, 1, node_id).await.unwrap_err();
            assert!(
                matches!(error, PluginError::InvalidConfig(_)),
                "{node_id:?}: {error:?}"
            );
        }
        assert!(stub.recorded().is_empty());
    }

    #[tokio::test]
    async fn a_record_flux_no_longer_keeps_is_not_found_but_the_object_events_still_read() {
        let dir = tempfile::tempdir().unwrap();
        let stub = cluster_with_events(vec![web_kustomization()], vec![], warning_events());
        let plugin = plugin(&stub.server, &dir, &[]);
        let metadata = HashMap::new();
        let run = RunRef {
            pipeline_id: LOG_APP,
            run_number: 42,
            run_id: "",
            metadata: &metadata,
        };

        let nodes = plugin.fetch_log_nodes(run).await.unwrap_err();
        let record = read_raw(&plugin, LOG_APP, 42, "reconciliation")
            .await
            .unwrap_err();
        let events = read_raw(&plugin, LOG_APP, 42, "object-events").await;

        assert!(
            matches!(nodes, PluginError::PipelineNotFound(_)),
            "{nodes:?}"
        );
        assert!(
            matches!(record, PluginError::PipelineNotFound(_)),
            "{record:?}"
        );
        assert_eq!(events.unwrap().lines().count(), 3);
    }

    #[tokio::test]
    async fn forbidden_events_are_a_permission_denial_naming_the_resource() {
        let dir = tempfile::tempdir().unwrap();
        let stub = stub(|_, _, _| {
            (
                403,
                status_json(
                    403,
                    "Forbidden",
                    "events is forbidden: cannot list resource",
                ),
            )
        });
        let plugin = plugin(&stub.server, &dir, &[]);

        let error = read_raw(&plugin, LOG_APP, 1, "object-events")
            .await
            .unwrap_err();

        assert!(
            matches!(&error, PluginError::PermissionDenied(message) if message.contains("list events of Kustomization ci/web")),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn events_permission_is_reviewed_in_every_monitored_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let stub = review_stub(|attributes| {
            attributes["resource"] == "events"
                && attributes["group"] == ""
                && attributes["verb"] == "list"
                && attributes["namespace"] == "ci"
        });
        let granted = |status: PermissionStatus| {
            status
                .permissions
                .into_iter()
                .find(|check| check.permission.name == "Read Flux object events")
                .map(|check| (check.granted, check.permission.required))
        };

        let both = plugin(&stub.server, &dir, &[("namespaces", "ci,apps")]);
        let only = plugin(&stub.server, &dir, &[("namespaces", "ci")]);

        assert_eq!(
            granted(both.check_permissions().await.unwrap()),
            Some((false, false))
        );
        assert_eq!(
            granted(only.check_permissions().await.unwrap()),
            Some((true, false))
        );
    }

    const KUSTOMIZE: &str = "kustomize.toolkit.fluxcd.io";
    const HELM: &str = "helm.toolkit.fluxcd.io";
    const APPS: &str = "/apis/apps/v1/";

    fn workload(group: &str, owner: &str, name: &str, spec: Value, status: Value) -> Value {
        let labels = HashMap::from([
            (format!("{group}/name"), owner),
            (format!("{group}/namespace"), "ci"),
        ]);

        json!({
            "metadata": { "name": name, "namespace": "apps", "generation": 1, "labels": labels },
            "spec": spec,
            "status": status,
        })
    }

    fn unready_deployment(owner: &str, name: &str) -> (&'static str, Value) {
        (
            "deployments",
            workload(
                KUSTOMIZE,
                owner,
                name,
                json!({ "replicas": 3 }),
                json!({ "observedGeneration": 1, "updatedReplicas": 3, "readyReplicas": 1 }),
            ),
        )
    }

    fn apps_stub(
        kustomizations: Vec<Value>, helm_releases: Vec<Value>,
        workloads: Vec<(&'static str, Value)>,
    ) -> Stub {
        stub(move |_, target, _| {
            let path = target.split('?').next().unwrap_or_default();
            let Some(plural) = path.strip_prefix(APPS) else {
                return serve(target, &kustomizations, &helm_releases);
            };
            let group = if target.contains(HELM) {
                HELM
            } else {
                KUSTOMIZE
            };
            let items = workloads
                .iter()
                .filter(|(kind, item)| {
                    *kind == plural
                        && item["metadata"]["labels"][format!("{group}/name")].is_string()
                })
                .map(|(_, item)| item.clone())
                .collect();

            (200, list(items, None))
        })
    }

    fn workload_requests(requests: &[Recorded]) -> Vec<String> {
        requests
            .iter()
            .filter(|(_, target, _)| target.starts_with(APPS))
            .map(|(_, target, _)| target.clone())
            .collect()
    }

    fn cluster_requests(requests: &[Recorded]) -> usize {
        requests.len() - workload_requests(requests).len()
    }

    fn without_grace(mut plugin: FluxcdPlugin) -> FluxcdPlugin {
        plugin.unhealthy =
            std::sync::Mutex::new(UnhealthyWorkloads::with_grace(std::time::Duration::ZERO));
        plugin
    }

    async fn pipeline_with(workloads: Vec<(&'static str, Value)>) -> Pipeline {
        let dir = tempfile::tempdir().unwrap();
        let stub = apps_stub(vec![kustomization("ci", "app")], vec![], workloads);
        let mut pipelines = without_grace(plugin(&stub.server, &dir, &[]))
            .fetch_pipelines()
            .await
            .unwrap();

        pipelines.remove(0)
    }

    fn assert_flux_ready(pipeline: &Pipeline) {
        assert_eq!(pipeline.status, PipelineStatus::Success);
        assert_eq!(
            pipeline.resource_state,
            Some(ResourceState::new(
                "Ready",
                Some("Applied revision: main@sha1:abc")
            ))
        );
    }

    #[tokio::test]
    async fn unhealthy_managed_workloads_degrade_only_the_resource_that_owns_them() {
        let dir = tempfile::tempdir().unwrap();
        let stub = apps_stub(
            vec![kustomization("ci", "app"), kustomization("ci", "other")],
            vec![helm_release("ci", "chart")],
            vec![
                unready_deployment("app", "web"),
                (
                    "daemonsets",
                    workload(
                        KUSTOMIZE,
                        "app",
                        "agent",
                        json!({}),
                        json!({
                            "observedGeneration": 1, "desiredNumberScheduled": 4,
                            "updatedNumberScheduled": 4, "numberUnavailable": 1,
                        }),
                    ),
                ),
                (
                    "deployments",
                    workload(
                        KUSTOMIZE,
                        "other",
                        "api",
                        json!({ "replicas": 2 }),
                        json!({ "observedGeneration": 1, "updatedReplicas": 2, "readyReplicas": 2 }),
                    ),
                ),
                (
                    "statefulsets",
                    workload(
                        HELM,
                        "chart",
                        "db",
                        json!({ "replicas": 1 }),
                        json!({
                            "observedGeneration": 1,
                            "currentRevision": "db-1", "updateRevision": "db-1",
                        }),
                    ),
                ),
            ],
        );
        let plugin = without_grace(plugin(&stub.server, &dir, &[]));

        let pipelines = plugin.fetch_pipelines().await.unwrap();

        let state = |name: &str| {
            let pipeline = pipelines.iter().find(|p| p.name == name).unwrap();
            let state = pipeline.resource_state.clone().unwrap();
            (pipeline.status, state.label, state.message.unwrap())
        };
        assert_eq!(
            state("Kustomization/app"),
            (
                PipelineStatus::Degraded,
                "Degraded".to_string(),
                "DaemonSet apps/agent: 3 of 4 pods available (+1 more)".to_string()
            )
        );
        assert_eq!(
            state("HelmRelease/chart"),
            (
                PipelineStatus::Degraded,
                "Degraded".to_string(),
                "StatefulSet apps/db: 0 of 1 replicas ready".to_string()
            )
        );
        let other = pipelines.iter().find(|p| p.name == "Kustomization/other");
        assert_flux_ready(other.unwrap());
    }

    #[tokio::test]
    async fn pods_still_warming_up_after_an_update_keep_the_ready_state() {
        let dir = tempfile::tempdir().unwrap();
        let stub = apps_stub(
            vec![kustomization("ci", "app")],
            vec![],
            vec![(
                "daemonsets",
                workload(
                    KUSTOMIZE,
                    "app",
                    "agent",
                    json!({}),
                    json!({
                        "observedGeneration": 1, "desiredNumberScheduled": 4,
                        "updatedNumberScheduled": 4, "numberUnavailable": 1,
                    }),
                ),
            )],
        );
        let plugin = plugin(&stub.server, &dir, &[]);

        for _ in 0..2 {
            let pipelines = plugin.fetch_pipelines().await.unwrap();
            assert_flux_ready(&pipelines[0]);
        }
    }

    #[tokio::test]
    async fn a_rollout_in_progress_keeps_the_ready_state() {
        let pipeline = pipeline_with(vec![
            (
                "deployments",
                workload(
                    KUSTOMIZE,
                    "app",
                    "web",
                    json!({ "replicas": 3 }),
                    json!({ "observedGeneration": 1, "updatedReplicas": 1, "readyReplicas": 2 }),
                ),
            ),
            (
                "statefulsets",
                workload(
                    KUSTOMIZE,
                    "app",
                    "db",
                    json!({ "replicas": 3 }),
                    json!({
                        "observedGeneration": 1, "readyReplicas": 1,
                        "currentRevision": "db-1", "updateRevision": "db-2",
                    }),
                ),
            ),
            (
                "daemonsets",
                workload(
                    KUSTOMIZE,
                    "app",
                    "agent",
                    json!({}),
                    json!({
                        "observedGeneration": 1, "desiredNumberScheduled": 4,
                        "updatedNumberScheduled": 2, "numberUnavailable": 2,
                    }),
                ),
            ),
        ])
        .await;

        assert_flux_ready(&pipeline);
    }

    #[tokio::test]
    async fn workloads_scaled_to_zero_keep_the_ready_state() {
        let pipeline = pipeline_with(vec![
            (
                "deployments",
                workload(
                    KUSTOMIZE,
                    "app",
                    "web",
                    json!({ "replicas": 0 }),
                    json!({ "observedGeneration": 1 }),
                ),
            ),
            (
                "statefulsets",
                workload(
                    KUSTOMIZE,
                    "app",
                    "db",
                    json!({ "replicas": 0 }),
                    json!({
                        "observedGeneration": 1,
                        "currentRevision": "db-1", "updateRevision": "db-1",
                    }),
                ),
            ),
        ])
        .await;

        assert_flux_ready(&pipeline);
    }

    #[tokio::test]
    async fn forbidden_workload_lists_keep_the_flux_reported_state() {
        let dir = tempfile::tempdir().unwrap();
        let all_forbidden = stub(|_, target, _| {
            if target.starts_with(APPS) {
                (
                    403,
                    status_json(403, "Forbidden", "deployments.apps is forbidden"),
                )
            } else {
                serve(target, &[kustomization("ci", "app")], &[])
            }
        });
        let only_deployments_forbidden = stub(|_, target, _| {
            if target.starts_with("/apis/apps/v1/deployments") {
                (
                    403,
                    status_json(403, "Forbidden", "deployments.apps is forbidden"),
                )
            } else if target.starts_with("/apis/apps/v1/statefulsets") {
                (
                    200,
                    list(
                        vec![workload(
                            KUSTOMIZE,
                            "app",
                            "db",
                            json!({ "replicas": 1 }),
                            json!({
                                "observedGeneration": 1,
                                "currentRevision": "db-1", "updateRevision": "db-1",
                            }),
                        )],
                        None,
                    ),
                )
            } else if target.starts_with(APPS) {
                (200, list(vec![], None))
            } else {
                serve(target, &[kustomization("ci", "app")], &[])
            }
        });

        let kept = plugin(&all_forbidden.server, &dir, &[])
            .fetch_pipelines()
            .await
            .unwrap();
        let partial = without_grace(plugin(&only_deployments_forbidden.server, &dir, &[]))
            .fetch_pipelines()
            .await
            .unwrap();

        assert_eq!(kept.len(), 1);
        assert_flux_ready(&kept[0]);
        assert_eq!(partial[0].status, PipelineStatus::Degraded);
    }

    #[tokio::test]
    async fn workload_lookups_are_a_fixed_number_of_lists_regardless_of_resource_count() {
        let dir = tempfile::tempdir().unwrap();
        let mut lookups = Vec::new();
        for resources in [1, 40] {
            let stub = apps_stub(
                (0..resources)
                    .map(|i| kustomization("ci", &format!("k{i}")))
                    .collect(),
                (0..resources)
                    .map(|i| helm_release("ci", &format!("h{i}")))
                    .collect(),
                vec![unready_deployment("k0", "web")],
            );

            let pipelines = plugin(&stub.server, &dir, &[])
                .fetch_pipelines()
                .await
                .unwrap();

            assert_eq!(pipelines.len(), 2 * resources);
            lookups.push(workload_requests(&stub.recorded()));
        }
        assert_eq!(lookups[0], lookups[1]);
        assert_eq!(lookups[0].len(), 6);
        for group in [KUSTOMIZE, HELM] {
            for plural in ["deployments", "statefulsets", "daemonsets"] {
                let found = lookups[0]
                    .iter()
                    .filter(|target| {
                        target.starts_with(&format!("{APPS}{plural}?"))
                            && target.contains("labelSelector=")
                            && target.contains(group)
                    })
                    .count();
                assert_eq!(found, 1, "{group} {plural}");
            }
        }

        let kustomizations_only = apps_stub(
            vec![kustomization("ci", "app"), kustomization("ci", "other")],
            vec![],
            vec![],
        );
        plugin(&kustomizations_only.server, &dir, &[])
            .fetch_pipelines()
            .await
            .unwrap();
        assert_eq!(workload_requests(&kustomizations_only.recorded()).len(), 3);
    }

    #[tokio::test]
    async fn only_ready_resources_trigger_workload_lookups() {
        let dir = tempfile::tempdir().unwrap();
        let failing = flux(
            "Kustomization",
            "ci",
            "broken",
            json!({}),
            json!({
                "observedGeneration": 1,
                "conditions": [{
                    "type": "Ready", "status": "False", "reason": "BuildFailed",
                    "message": "build failed", "observedGeneration": 1,
                    "lastTransitionTime": "2026-10-20T10:00:05Z",
                }],
            }),
        );
        let stub = apps_stub(
            vec![failing],
            vec![],
            vec![unready_deployment("broken", "web")],
        );

        let pipelines = plugin(&stub.server, &dir, &[])
            .fetch_pipelines()
            .await
            .unwrap();

        assert_eq!(pipelines[0].status, PipelineStatus::Failed);
        assert_eq!(
            pipelines[0].resource_state,
            Some(ResourceState::new("Not ready", Some("build failed")))
        );
        assert!(workload_requests(&stub.recorded()).is_empty());
    }
}
