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
    AvailablePipeline,
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
    mapper,
    metadata::{
        self,
        PERMISSIONS,
    },
    types::{
        FluxResource,
        Kind,
    },
};

const CONCURRENCY: usize = 8;

pub struct FluxcdPlugin {
    metadata: PluginMetadata,
    client: OnceCell<FluxClient>,
    provider_id: Option<i64>,
    settings: Option<Settings>,
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
        metadata: HashMap::from([
            ("kind".to_string(), json!(target.kind.as_str())),
            ("namespace".to_string(), json!(target.namespace)),
            ("name".to_string(), json!(target.name)),
            ("missing".to_string(), json!(true)),
            ("reason".to_string(), json!("ResourceNotFound")),
            ("status_label".to_string(), json!("Not found")),
            ("message".to_string(), json!(message)),
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
                FluxClient::connect(settings.kubeconfig_paths.clone(), settings.context.clone())
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

    async fn runs(&self, pipeline_id: &str) -> PluginResult<(Target, Vec<PipelineRun>)> {
        let target = self.target(pipeline_id)?;
        let resource = self.client().await?.get(&target).await?;
        let runs = mapper::build_runs(&resource, self.provider_id()?)?;

        Ok((target, runs))
    }

    fn scopes(&self) -> PluginResult<Vec<Option<String>>> {
        let namespaces = self.settings()?.monitored_namespaces();

        Ok(if namespaces.is_empty() {
            vec![None]
        } else {
            namespaces.into_iter().map(Some).collect()
        })
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
        self.settings = Some(Settings::from_config(&config)?);
        self.provider_id = Some(provider_id);
        self.client = OnceCell::new();

        Ok(())
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

        Ok(found
            .iter()
            .map(|resource| mapper::map_pipeline(resource, provider_id))
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
        let (_, runs) = self.runs(pipeline_id).await?;

        Ok(runs.into_iter().take(limit).collect())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let (target, runs) = self.runs(pipeline_id).await?;
        let mut matching: Vec<PipelineRun> = runs
            .into_iter()
            .filter(|run| run.run_number == run_number)
            .collect();

        match matching.len() {
            0 => Err(PluginError::PipelineNotFound(format!(
                "{target} has no record {run_number}. Flux keeps a bounded status.history, so older records are no longer available"
            ))),
            1 => Ok(matching.remove(0)),
            _ => Err(PluginError::ApiError(format!(
                "{target} has {} records numbered {run_number}; refusing to pick one",
                matching.len()
            ))),
        }
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
            "context" => client::available_contexts(config::kubeconfig_paths(config)).await,
            "namespaces" => {
                let client =
                    FluxClient::connect(config::kubeconfig_paths(config), config::context(config))
                        .await?;

                match client.namespaces().await {
                    Err(PluginError::AuthenticationFailed(reason)) => {
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
        let checks: Vec<(usize, &'static str, Kind, Option<String>)> = PERMISSIONS
            .iter()
            .enumerate()
            .flat_map(|(index, spec)| {
                let scopes = &scopes;
                spec.verbs.iter().flat_map(move |verb| {
                    scopes
                        .iter()
                        .map(move |scope| (index, *verb, spec.kind, scope.clone()))
                })
            })
            .collect();
        let mut reviews = Vec::with_capacity(checks.len());
        for batch in checks.chunks(CONCURRENCY) {
            let results = futures::future::try_join_all(batch.iter().map(
                |(index, verb, kind, scope)| async move {
                    let allowed = client.can_i(verb, *kind, scope.as_deref()).await?;
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
        assert_eq!(stub.recorded().len(), 1);
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
        assert_eq!(missing.metadata["namespace"], json!("ci"));
        assert_eq!(missing.metadata["name"], json!("gone"));
        assert!(!missing.metadata.contains_key("uid"));
        assert_eq!(stub.recorded().len(), 1);
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
            matches!(&error, PluginError::AuthenticationFailed(message) if message.contains("namespace ci") && message.contains("cannot list resource")),
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
    async fn forbidden_requests_are_authentication_failures_naming_the_resource() {
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
            matches!(&error, PluginError::AuthenticationFailed(message) if message.contains("Kustomization ci/app") && message.contains("cannot patch")),
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
            matches!(&error, PluginError::AuthenticationFailed(message) if message.contains("namespace denied") && message.contains("cannot list resource")),
            "{error:?}"
        );
    }

    #[tokio::test]
    async fn missing_and_failing_resources_report_not_found_and_api_errors_with_names() {
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
            matches!(&failing, PluginError::ApiError(message) if message.contains("HelmRelease ci/web") && message.contains("etcd unavailable")),
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
            matches!(denied, Err(PluginError::AuthenticationFailed(_))),
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
            ]
        );
        assert!(!status.all_granted);
        assert_eq!(status.metadata["scope"], "namespaces: ci");
        let reviewed = stub.recorded();
        assert_eq!(reviewed.len(), 2 * 2 + 2);
        assert!(reviewed
            .iter()
            .all(|(method, _, body)| method == "POST" && body.contains("\"namespace\":\"ci\"")));
    }

    #[tokio::test]
    async fn list_only_access_is_not_reported_as_read_access() {
        let dir = tempfile::tempdir().unwrap();
        let stub = review_stub(|attributes| attributes["verb"] == "list");
        let plugin = plugin(&stub.server, &dir, &[]);

        let status = plugin.check_permissions().await.unwrap();

        assert!(status.permissions.iter().all(|check| !check.granted));
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
            matches!(&result, Err(PluginError::ApiError(message)) if message.contains("review failed")),
            "{result:?}"
        );
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
}
