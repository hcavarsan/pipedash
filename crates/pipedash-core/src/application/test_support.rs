use std::collections::HashMap;
use std::sync::atomic::{
    AtomicBool,
    AtomicU64,
    AtomicUsize,
    Ordering,
};
use std::sync::{
    Arc,
    Mutex,
};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use pipedash_plugin_api::{
    ComponentHealth,
    PluginCapabilities,
    RateLimit,
    StatusSource,
    WorkflowParameter,
};
use sqlx::SqlitePool;
use tempfile::TempDir;
use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};
use tokio::sync::{
    mpsc,
    watch,
    Notify,
};
use tokio::time::Instant;

use super::{
    FetchResult,
    FetchScheduler,
    ForcedClaim,
    HealthHandle,
    HealthInput,
    MetricsService,
    PipelineService,
    ProviderHealthService,
    ProviderService,
    RefreshDispatcher,
    RefreshManager,
    RefreshMode,
};
use crate::domain::{
    DomainError,
    DomainResult,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    Provider,
    ProviderConfig,
    TriggerParams,
};
use crate::event::{
    CoreEvent,
    EventBus,
};
use crate::infrastructure::database::{
    init_database,
    DatabasePool,
    MetricsRepository,
    Repository,
};
use crate::infrastructure::{
    ConfigState,
    HttpClientManager,
    MemoryTokenStore,
    PipedashConfig,
    SqliteConfigBackend,
    TokenStore,
};

pub(crate) struct Gate {
    arrived: Notify,
    open: watch::Sender<bool>,
}

impl Gate {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            arrived: Notify::new(),
            open: watch::channel(false).0,
        })
    }

    async fn pass(&self) {
        self.arrived.notify_one();
        let mut open = self.open.subscribe();
        let _ = open.wait_for(|open| *open).await;
    }

    pub(crate) async fn arrived(&self) {
        tokio::time::timeout(Duration::from_secs(10), self.arrived.notified())
            .await
            .expect("nothing reached the gate");
    }

    pub(crate) fn open(&self) {
        self.open.send_replace(true);
    }

    pub(crate) fn close(&self) {
        self.open.send_replace(false);
    }
}

pub(crate) fn pipeline(provider_id: i64, name: &str, status: PipelineStatus) -> Pipeline {
    Pipeline {
        id: format!("github__{provider_id}__{name}"),
        provider_id,
        provider_type: "github".to_string(),
        name: name.to_string(),
        status,
        last_run: None,
        last_updated: Utc::now(),
        repository: "acme/app".to_string(),
        branch: None,
        workflow_file: None,
        metadata: HashMap::new(),
        overdue_run: None,
    }
}

pub(crate) fn stub_run(pipeline_id: &str, number: i64, duration_seconds: i64) -> PipelineRun {
    serde_json::from_value(serde_json::json!({
        "id": format!("{pipeline_id}__{number}"), "pipeline_id": pipeline_id,
        "run_number": number, "status": "success", "started_at": Utc::now(),
        "duration_seconds": duration_seconds, "logs_url": ""
    }))
    .unwrap()
}

pub(crate) fn parameter(name: &str) -> WorkflowParameter {
    serde_json::from_value(serde_json::json!({ "name": name, "type": "string" })).unwrap()
}

pub(crate) struct StubProvider {
    pub(crate) provider_id: i64,
    pub(crate) gate: Option<Arc<Gate>>,
    pub(crate) run_gate: Option<Arc<Gate>>,
    pub(crate) parameter_gate: Option<Arc<Gate>>,
    pub(crate) fetches: AtomicUsize,
    pub(crate) run_fetches: AtomicUsize,
    pub(crate) parameter_fetches: AtomicUsize,
    pub(crate) pipelines: Mutex<DomainResult<Vec<Pipeline>>>,
    pub(crate) runs: Mutex<Vec<PipelineRun>>,
    pub(crate) run_response: Mutex<Option<DomainResult<PipelineRun>>>,
    pub(crate) parameters: Mutex<DomainResult<Vec<WorkflowParameter>>>,
    pub(crate) rate_limit: Mutex<Option<RateLimit>>,
    pub(crate) capabilities: PluginCapabilities,
    pub(crate) source: Option<StatusSource>,
    pub(crate) health: Mutex<DomainResult<Vec<ComponentHealth>>>,
    pub(crate) health_gate: Option<Arc<Gate>>,
    pub(crate) health_checks: AtomicUsize,
}

impl StubProvider {
    pub(crate) fn new(provider_id: i64) -> Self {
        Self {
            provider_id,
            gate: None,
            run_gate: None,
            parameter_gate: None,
            fetches: AtomicUsize::new(0),
            run_fetches: AtomicUsize::new(0),
            parameter_fetches: AtomicUsize::new(0),
            pipelines: Mutex::new(Ok(vec![pipeline(
                provider_id,
                "web",
                PipelineStatus::Success,
            )])),
            runs: Mutex::new(Vec::new()),
            run_response: Mutex::new(None),
            parameters: Mutex::new(Ok(vec![parameter("branch")])),
            rate_limit: Mutex::new(None),
            capabilities: PluginCapabilities::default(),
            source: None,
            health: Mutex::new(Ok(Vec::new())),
            health_gate: None,
            health_checks: AtomicUsize::new(0),
        }
    }

    pub(crate) fn gated(mut self, gate: &Arc<Gate>) -> Self {
        self.gate = Some(Arc::clone(gate));
        self
    }

    pub(crate) fn run_gated(mut self, gate: &Arc<Gate>) -> Self {
        self.run_gate = Some(Arc::clone(gate));
        self
    }

    pub(crate) fn parameter_gated(mut self, gate: &Arc<Gate>) -> Self {
        self.parameter_gate = Some(Arc::clone(gate));
        self
    }

    pub(crate) fn retaining_run_history(mut self) -> Self {
        self.capabilities.retains_run_history = true;
        self
    }

    pub(crate) fn returning(self, pipelines: Vec<Pipeline>) -> Self {
        *self.pipelines.lock().unwrap() = Ok(pipelines);
        self
    }

    pub(crate) fn failing(self, error: DomainError) -> Self {
        *self.pipelines.lock().unwrap() = Err(error);
        self
    }

    pub(crate) fn with_runs(self, runs: Vec<PipelineRun>) -> Self {
        *self.runs.lock().unwrap() = runs;
        self
    }

    pub(crate) fn set_pipelines(&self, pipelines: Vec<Pipeline>) {
        *self.pipelines.lock().unwrap() = Ok(pipelines);
    }

    pub(crate) fn fetch_count(&self) -> usize {
        self.fetches.load(Ordering::SeqCst)
    }

    pub(crate) fn with_source(mut self, source: StatusSource) -> Self {
        self.source = Some(source);
        self
    }

    pub(crate) fn health_gated(mut self, gate: &Arc<Gate>) -> Self {
        self.health_gate = Some(Arc::clone(gate));
        self
    }

    pub(crate) fn set_health(&self, health: DomainResult<Vec<ComponentHealth>>) {
        *self.health.lock().unwrap() = health;
    }

    pub(crate) fn health_check_count(&self) -> usize {
        self.health_checks.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Provider for StubProvider {
    async fn fetch_pipelines(&self) -> DomainResult<Vec<Pipeline>> {
        self.fetches.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = &self.gate {
            gate.pass().await;
        }
        self.pipelines.lock().unwrap().clone()
    }

    async fn fetch_run_history(
        &self, _pipeline_id: &str, _limit: usize,
    ) -> DomainResult<Vec<PipelineRun>> {
        if let Some(gate) = &self.run_gate {
            gate.pass().await;
        }
        Ok(self.runs.lock().unwrap().clone())
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<PipelineRun> {
        self.run_fetches.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = &self.run_gate {
            gate.pass().await;
        }
        if let Some(response) = self.run_response.lock().unwrap().clone() {
            return response;
        }
        self.runs
            .lock()
            .unwrap()
            .iter()
            .find(|run| run.pipeline_id == pipeline_id && run.run_number == run_number)
            .cloned()
            .ok_or_else(|| DomainError::PipelineNotFound(pipeline_id.to_string()))
    }

    async fn trigger_pipeline(&self, _params: TriggerParams) -> DomainResult<String> {
        Err(DomainError::NotSupported("stub".to_string()))
    }

    async fn cancel_run(&self, _pipeline_id: &str, _run_number: i64) -> DomainResult<()> {
        Err(DomainError::NotSupported("stub".to_string()))
    }

    async fn review_run(
        &self, _pipeline_id: &str, _run_number: i64, _decision: pipedash_plugin_api::ReviewDecision,
    ) -> DomainResult<()> {
        Err(DomainError::NotSupported("stub".to_string()))
    }

    async fn rerun_failed_jobs(&self, _pipeline_id: &str, _run_number: i64) -> DomainResult<()> {
        Err(DomainError::NotSupported("stub".to_string()))
    }

    async fn get_workflow_parameters(
        &self, _workflow_id: &str,
    ) -> DomainResult<Vec<WorkflowParameter>> {
        self.parameter_fetches.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = &self.parameter_gate {
            gate.pass().await;
        }
        self.parameters.lock().unwrap().clone()
    }

    async fn fetch_rate_limit(&self) -> DomainResult<Option<RateLimit>> {
        Ok(self.rate_limit.lock().unwrap().clone())
    }

    async fn validate_credentials(&self) -> DomainResult<bool> {
        Ok(true)
    }

    fn provider_type(&self) -> &str {
        "github"
    }

    fn capabilities(&self) -> &PluginCapabilities {
        &self.capabilities
    }

    fn status_source(&self) -> Option<&StatusSource> {
        self.source.as_ref()
    }

    async fn check_health(&self) -> DomainResult<Vec<ComponentHealth>> {
        self.health_checks.fetch_add(1, Ordering::SeqCst);
        if let Some(gate) = &self.health_gate {
            gate.pass().await;
        }
        self.health.lock().unwrap().clone()
    }
}

#[derive(Default)]
pub(crate) struct StubFactory {
    queued: Mutex<HashMap<i64, Vec<Arc<StubProvider>>>>,
    built: Mutex<HashMap<i64, usize>>,
    tokens: Mutex<HashMap<i64, String>>,
}

impl StubFactory {
    pub(crate) fn queue(&self, provider: StubProvider) -> Arc<StubProvider> {
        let provider = Arc::new(provider);
        self.queued
            .lock()
            .unwrap()
            .entry(provider.provider_id)
            .or_default()
            .push(Arc::clone(&provider));
        provider
    }

    pub(crate) fn provide(&self, config: &ProviderConfig) -> DomainResult<Arc<dyn Provider>> {
        let id = config.id.unwrap_or(0);
        self.tokens.lock().unwrap().insert(id, config.token.clone());
        let mut built = self.built.lock().unwrap();
        let index = built.entry(id).or_default();
        let queued = self.queued.lock().unwrap();
        let provider = match queued.get(&id) {
            Some(providers) => Arc::clone(&providers[(*index).min(providers.len() - 1)]),
            None => Arc::new(StubProvider::new(id)),
        };
        *index += 1;

        Ok(provider)
    }

    pub(crate) fn token_of(&self, id: i64) -> Option<String> {
        self.tokens.lock().unwrap().get(&id).cloned()
    }
}

#[derive(Default)]
pub(crate) struct FailOnceTokenStore {
    inner: MemoryTokenStore,
    fail_next_store: AtomicBool,
    fail_next_delete: AtomicBool,
    delete_gate: Mutex<Option<Arc<Gate>>>,
}

impl FailOnceTokenStore {
    pub(crate) fn fail_next_store(&self) {
        self.fail_next_store.store(true, Ordering::SeqCst);
    }

    pub(crate) fn fail_next_delete(&self) {
        self.fail_next_delete.store(true, Ordering::SeqCst);
    }

    pub(crate) fn gate_delete(&self, gate: Arc<Gate>) {
        *self.delete_gate.lock().unwrap() = Some(gate);
    }
}

#[async_trait]
impl TokenStore for FailOnceTokenStore {
    async fn store_token(&self, provider_id: i64, token: &str) -> DomainResult<()> {
        if self.fail_next_store.swap(false, Ordering::SeqCst) {
            return Err(DomainError::DatabaseError(
                "token store unavailable".to_string(),
            ));
        }
        self.inner.store_token(provider_id, token).await
    }

    async fn get_token(&self, provider_id: i64) -> DomainResult<String> {
        self.inner.get_token(provider_id).await
    }

    async fn delete_token(&self, provider_id: i64) -> DomainResult<()> {
        if self.fail_next_delete.swap(false, Ordering::SeqCst) {
            return Err(DomainError::DatabaseError(
                "token deletion unavailable".into(),
            ));
        }
        let gate = self.delete_gate.lock().unwrap().clone();
        if let Some(gate) = gate {
            gate.pass().await;
        }
        self.inner.delete_token(provider_id).await
    }

    async fn get_all_tokens(&self) -> DomainResult<HashMap<i64, String>> {
        self.inner.get_all_tokens().await
    }
}

#[derive(Default)]
pub(crate) struct RecordingBus {
    events: Mutex<Vec<CoreEvent>>,
}

impl RecordingBus {
    pub(crate) fn events(&self) -> Vec<CoreEvent> {
        self.events.lock().unwrap().clone()
    }

    pub(crate) fn names(&self) -> Vec<&'static str> {
        self.events().iter().map(CoreEvent::event_name).collect()
    }

    pub(crate) fn count(&self, name: &str) -> usize {
        self.names().iter().filter(|event| **event == name).count()
    }

    pub(crate) async fn wait_for(&self, what: &str, found: impl Fn(&[CoreEvent]) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !found(&self.events()) {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

#[async_trait]
impl EventBus for RecordingBus {
    async fn emit(&self, event: CoreEvent) {
        self.events.lock().unwrap().push(event);
    }

    async fn emit_to(&self, _target: &str, event: CoreEvent) {
        self.events.lock().unwrap().push(event);
    }
}

pub(crate) struct Harness {
    pub(crate) _temp_dir: TempDir,
    pub(crate) token_store: Arc<FailOnceTokenStore>,
    pub(crate) pool: SqlitePool,
    pub(crate) repository: Arc<Repository>,
    pub(crate) bus: Arc<RecordingBus>,
    pub(crate) scheduler: Arc<FetchScheduler>,
    pub(crate) health: HealthHandle,
    pub(crate) health_service: Option<Arc<ProviderHealthService>>,
    pub(crate) config_state: Arc<ConfigState>,
    health_inputs: Option<Mutex<mpsc::UnboundedReceiver<HealthInput>>>,
    pub(crate) provider_service: Arc<ProviderService>,
    pub(crate) pipeline_service: Arc<PipelineService>,
    pub(crate) refresh_manager: Arc<RefreshManager>,
    pub(crate) metrics: Option<Arc<MetricsService>>,
    pub(crate) stubs: Arc<StubFactory>,
    pub(crate) argocd_url: String,
    clock: AtomicU64,
    names: AtomicU64,
}

impl Harness {
    pub(crate) async fn new() -> Self {
        Self::build(true, false, false).await
    }

    pub(crate) async fn with_metrics() -> Self {
        Self::build(true, true, false).await
    }

    pub(crate) async fn with_real_plugins() -> Self {
        Self::build(false, false, false).await
    }

    pub(crate) async fn with_health() -> Self {
        Self::build(true, false, true).await
    }

    async fn build(stubbed: bool, metrics: bool, actor: bool) -> Self {
        let temp_dir = TempDir::new().unwrap();
        let pool = init_database(temp_dir.path().join("harness.db"))
            .await
            .unwrap();
        let token_store = Arc::new(FailOnceTokenStore::default());
        let repository = Arc::new(Repository::new(
            Arc::new(SqliteConfigBackend::new(pool.clone())),
            DatabasePool::Sqlite(pool.clone()),
            token_store.clone(),
        ));
        let metrics = metrics.then(|| {
            Arc::new(MetricsService::new(Arc::new(MetricsRepository::new(
                pool.clone(),
            ))))
        });
        let bus = Arc::new(RecordingBus::default());
        let scheduler = Arc::new(FetchScheduler::default());
        let (health, receivers) = HealthHandle::channel();
        let (dispatcher, commands) = RefreshDispatcher::channel();
        let (mode_tx, mode_rx) = watch::channel(RefreshMode::Active);
        let stubs = Arc::new(StubFactory::default());
        let http = Arc::new(HttpClientManager::new().unwrap());
        let config_state = ConfigState::from_config(
            PipedashConfig::default(),
            temp_dir.path().join("config.toml"),
            token_store.clone(),
        );
        config_state.attach_health(health.clone());

        let mut provider_service = ProviderService::new(
            Arc::clone(&repository),
            Arc::clone(&http),
            bus.clone(),
            Arc::clone(&scheduler),
            health.clone(),
            dispatcher,
            Arc::clone(&config_state),
        );
        if stubbed {
            let stubs = Arc::clone(&stubs);
            provider_service = provider_service
                .with_provider_factory(Arc::new(move |config| stubs.provide(config)));
        }
        let provider_service = Arc::new(provider_service);
        let (health_service, health_inputs) = if actor {
            let service = ProviderHealthService::new(
                receivers,
                Arc::clone(&provider_service),
                http,
                bus.clone(),
                Arc::clone(&config_state),
                mode_tx.subscribe(),
            );
            (Some(Arc::new(service)), None)
        } else {
            (None, Some(Mutex::new(receivers.inputs)))
        };
        let pipeline_service = Arc::new(PipelineService::new(
            Arc::clone(&repository),
            Arc::clone(&provider_service),
            metrics.clone(),
            bus.clone(),
            mode_rx,
        ));
        let refresh_manager = Arc::new(RefreshManager::new(
            Arc::clone(&pipeline_service),
            metrics.clone(),
            mode_tx,
            commands,
        ));

        Self {
            _temp_dir: temp_dir,
            token_store,
            pool,
            repository,
            bus,
            scheduler,
            health,
            health_service,
            config_state,
            health_inputs,
            provider_service,
            pipeline_service,
            refresh_manager,
            metrics,
            stubs,
            argocd_url: json_stub(serde_json::json!({ "items": [] })).await,
            clock: AtomicU64::new(0),
            names: AtomicU64::new(0),
        }
    }

    pub(crate) async fn start(&self) {
        self.refresh_manager.start().await;
    }

    pub(crate) fn config(&self, name: &str, refresh_interval: i64) -> ProviderConfig {
        ProviderConfig {
            id: None,
            name: name.to_string(),
            provider_type: "argocd".to_string(),
            token: "token".to_string(),
            config: HashMap::from([("server_url".to_string(), self.argocd_url.clone())]),
            refresh_interval,
            version: None,
        }
    }

    pub(crate) async fn add_provider(&self, name: &str) -> i64 {
        self.repository
            .add_provider(&self.config(name, 30))
            .await
            .unwrap()
    }

    pub(crate) async fn add_stub(
        &self, build: impl FnOnce(i64) -> StubProvider,
    ) -> (i64, Arc<StubProvider>) {
        self.add_stub_every(30, build).await
    }

    pub(crate) async fn add_stub_every(
        &self, refresh_interval: i64, build: impl FnOnce(i64) -> StubProvider,
    ) -> (i64, Arc<StubProvider>) {
        let name = format!("stub-{}", self.names.fetch_add(1, Ordering::SeqCst));
        let id = self
            .repository
            .add_provider(&self.config(&name, refresh_interval))
            .await
            .unwrap();

        (id, self.stubs.queue(build(id)))
    }

    pub(crate) async fn spawn_fetch(&self, id: i64) -> tokio::task::JoinHandle<FetchResult> {
        let lease = self.provider_service.acquire(id).await.unwrap();
        let offset = self.clock.fetch_add(60, Ordering::SeqCst) + 60;
        let ForcedClaim::Claimed(claim) = self.scheduler.claim_forced(
            id,
            lease.generation,
            Instant::now() + Duration::from_secs(offset),
        ) else {
            panic!("expected a fresh claim for provider {id}");
        };
        let service = Arc::clone(&self.pipeline_service);

        tokio::spawn(async move {
            service
                .fetch_provider(claim, super::ClaimKind::Forced)
                .await
        })
    }

    pub(crate) async fn fetch_directly(&self, id: i64) -> FetchResult {
        self.spawn_fetch(id).await.await.unwrap()
    }

    pub(crate) fn health_inputs(&self) -> Vec<HealthInput> {
        let Some(inputs) = &self.health_inputs else {
            return Vec::new();
        };
        let mut inputs = inputs.lock().unwrap();
        std::iter::from_fn(|| inputs.try_recv().ok()).collect()
    }

    pub(crate) async fn cached_names(&self, id: i64) -> Vec<String> {
        let mut names: Vec<String> = self
            .repository
            .get_cached_pipelines(Some(id))
            .await
            .unwrap()
            .into_iter()
            .map(|pipeline| pipeline.name)
            .collect();
        names.sort();
        names
    }
}

pub(crate) async fn json_stub(body: serde_json::Value) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let body = body.to_string();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let body = body.clone();
            tokio::spawn(async move {
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    match stream.read(&mut buffer).await {
                        Ok(0) | Err(_) => return,
                        Ok(count) => request.extend_from_slice(&buffer[..count]),
                    }
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });

    format!("http://{addr}")
}

#[derive(Debug, Clone)]
pub(crate) struct FeedRequest {
    pub(crate) path: String,
    pub(crate) if_none_match: Option<String>,
    pub(crate) accept: Option<String>,
    pub(crate) at: Instant,
}

struct StubResponse {
    status: u16,
    body: Vec<u8>,
    headers: Vec<(String, String)>,
}

struct StubState {
    response: StubResponse,
    etag: Option<String>,
    gate: Option<Arc<Gate>>,
    requests: Vec<FeedRequest>,
}

pub(crate) struct FeedStub {
    pub(crate) base_url: String,
    state: Arc<Mutex<StubState>>,
}

impl FeedStub {
    pub(crate) async fn start() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(StubState {
            response: StubResponse {
                status: 200,
                body: statuspage_body(&[("web", "operational")], serde_json::json!([]))
                    .to_string()
                    .into_bytes(),
                headers: Vec::new(),
            },
            etag: None,
            gate: None,
            requests: Vec::new(),
        }));
        let shared = Arc::clone(&state);
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(serve_feed(stream, Arc::clone(&shared)));
            }
        });

        Self { base_url, state }
    }

    pub(crate) fn feed_url(&self) -> reqwest::Url {
        reqwest::Url::parse(&format!("{}/api/v2/summary.json", self.base_url)).unwrap()
    }

    pub(crate) fn respond(&self, status: u16, body: impl Into<Vec<u8>>) {
        let mut state = self.state.lock().unwrap();
        state.response = StubResponse {
            status,
            body: body.into(),
            headers: Vec::new(),
        };
    }

    pub(crate) fn respond_json(&self, body: serde_json::Value) {
        self.respond(200, body.to_string());
    }

    pub(crate) fn with_header(&self, name: &str, value: &str) {
        self.state
            .lock()
            .unwrap()
            .response
            .headers
            .push((name.to_string(), value.to_string()));
    }

    pub(crate) fn set_etag(&self, etag: Option<&str>) {
        self.state.lock().unwrap().etag = etag.map(str::to_string);
    }

    pub(crate) fn set_gate(&self, gate: Option<&Arc<Gate>>) {
        self.state.lock().unwrap().gate = gate.map(Arc::clone);
    }

    pub(crate) fn requests(&self) -> Vec<FeedRequest> {
        self.state.lock().unwrap().requests.clone()
    }

    pub(crate) fn request_count(&self) -> usize {
        self.state.lock().unwrap().requests.len()
    }
}

async fn serve_feed(mut stream: tokio::net::TcpStream, state: Arc<Mutex<StubState>>) {
    let mut request = Vec::new();
    let mut buffer = [0; 4096];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        match stream.read(&mut buffer).await {
            Ok(0) | Err(_) => return,
            Ok(count) => request.extend_from_slice(&buffer[..count]),
        }
    }
    let head = String::from_utf8_lossy(&request).to_string();
    let mut lines = head.lines();
    let path = lines
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or_default()
        .to_string();
    let header = |name: &str| {
        head.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.trim()
                .eq_ignore_ascii_case(name)
                .then(|| value.trim().to_string())
        })
    };
    let if_none_match = header("if-none-match");
    let gate = {
        let mut state = state.lock().unwrap();
        state.requests.push(FeedRequest {
            path,
            if_none_match: if_none_match.clone(),
            accept: header("accept"),
            at: Instant::now(),
        });
        state.gate.clone()
    };
    if let Some(gate) = gate {
        gate.pass().await;
    }

    let (status, body, mut headers) = {
        let state = state.lock().unwrap();
        let matches = state.etag.is_some() && state.etag == if_none_match;
        let mut headers = state.response.headers.clone();
        if let Some(etag) = &state.etag {
            headers.push(("ETag".to_string(), etag.clone()));
        }
        if matches && state.response.status == 200 {
            (304, Vec::new(), headers)
        } else {
            (state.response.status, state.response.body.clone(), headers)
        }
    };
    headers.push(("Content-Type".to_string(), "application/json".to_string()));
    headers.push(("Content-Length".to_string(), body.len().to_string()));
    headers.push(("Connection".to_string(), "close".to_string()));
    let mut response = format!("HTTP/1.1 {status} Stub\r\n");
    for (name, value) in headers {
        response.push_str(&format!("{name}: {value}\r\n"));
    }
    response.push_str("\r\n");
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.write_all(&body).await;
    let _ = stream.shutdown().await;
}

pub(crate) fn statuspage_body(
    components: &[(&str, &str)], incidents: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "components": components
            .iter()
            .map(|(id, status)| serde_json::json!({
                "id": id, "name": format!("Component {id}"), "status": status
            }))
            .collect::<Vec<_>>(),
        "incidents": incidents,
        "scheduled_maintenances": [],
    })
}
