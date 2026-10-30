use std::collections::{
    HashMap,
    HashSet,
};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use futures::{
    StreamExt,
    TryStreamExt,
};
use pipedash_plugin_api::*;
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::{
    json,
    Value,
};
use tokio::sync::OnceCell;
use tracing::{
    debug,
    info,
    warn,
};

use crate::config::{
    self,
    Bucket,
    HistorySource,
    KubeSettings,
    Scope,
    Selection,
};
use crate::deck::{
    DeckClient,
    RerunMode,
};
use crate::mapper::{
    self,
    Latest,
};
use crate::metadata;
use crate::prowjobs::ProwJobClient;
use crate::sign_in;
use crate::storage::{
    self,
    Artifacts,
    HistoryRoot,
    StorageClient,
    StoredBuild,
};
use crate::types::{
    run_number_of,
    ProwJob,
};

const DEFAULT_PAGE_SIZE: usize = 100;
const HISTORY_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;
const HISTORY_WINDOW_GROWTH: i64 = 4;
const HISTORY_CONCURRENCY: usize = 4;
const READ_ONLY: &str = "Prow is connected without credentials, which is read-only";

impl LogSource {
    fn other(self) -> Self {
        match self {
            Self::Deck => Self::Storage,
            Self::Storage => Self::Deck,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum LogSource {
    Deck,
    Storage,
}

#[derive(Serialize, Deserialize)]
struct LogPosition {
    source: LogSource,
    offset: u64,
}

struct RunTarget {
    job: String,
    build_id: String,
    state: Option<String>,
    artifacts: Option<Artifacts>,
    status: PipelineStatus,
    web_url: Option<String>,
}

pub struct ProwPlugin {
    metadata: PluginMetadata,
    provider_id: Option<i64>,
    deck: Option<Arc<DeckClient>>,
    storage: Option<StorageClient>,
    history: HistorySource,
    storage_bucket: Option<Bucket>,
    selections: Vec<Selection>,
    read_only: bool,
    kube_settings: Option<KubeSettings>,
    kube: OnceCell<ProwJobClient>,
    status_source: Option<StatusSource>,
}

impl Default for ProwPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl ProwPlugin {
    pub fn new() -> Self {
        Self {
            metadata: metadata::create_metadata(),
            provider_id: None,
            deck: None,
            storage: None,
            history: HistorySource::Deck,
            storage_bucket: None,
            selections: Vec::new(),
            read_only: false,
            kube_settings: None,
            kube: OnceCell::new(),
            status_source: None,
        }
    }

    fn deck(&self) -> PluginResult<&DeckClient> {
        self.deck
            .as_deref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }

    fn storage(&self) -> PluginResult<&StorageClient> {
        self.storage
            .as_ref()
            .ok_or_else(|| PluginError::Internal("Plugin not initialized".to_string()))
    }

    fn provider_id(&self) -> PluginResult<i64> {
        self.provider_id
            .ok_or_else(|| PluginError::Internal("Provider ID not set".to_string()))
    }

    fn writable(&self) -> PluginResult<()> {
        if self.read_only {
            return Err(PluginError::NotSupported(READ_ONLY.to_string()));
        }
        Ok(())
    }

    async fn kube(&self) -> PluginResult<Option<&ProwJobClient>> {
        let Some(settings) = &self.kube_settings else {
            return Ok(None);
        };
        self.kube
            .get_or_try_init(|| ProwJobClient::connect(settings))
            .await
            .map(Some)
    }

    async fn scope_jobs(&self, scope: Option<&Scope>) -> PluginResult<Vec<ProwJob>> {
        self.deck()?
            .jobs(
                scope.map(|scope| scope.org.as_str()),
                scope.map(|scope| scope.repo.as_str()),
            )
            .await
    }

    fn history_location(&self, newest: Option<&ProwJob>) -> Option<(Bucket, Vec<HistoryRoot>)> {
        if let Some(job) = newest {
            if let Some(artifacts) = Artifacts::from_view_url(&job.status.url) {
                return Some((
                    Bucket {
                        provider: artifacts.provider,
                        name: artifacts.bucket,
                    },
                    vec![HistoryRoot::for_job_type(&job.spec.job_type)],
                ));
            }
        }
        self.storage_bucket
            .clone()
            .map(|bucket| (bucket, vec![HistoryRoot::Logs, HistoryRoot::PullDirectory]))
    }

    async fn stored_latest(&self, job: &str) -> PluginResult<Option<StoredBuild>> {
        let Some((bucket, roots)) = self.history_location(None) else {
            return Ok(None);
        };
        let storage = self.storage()?;
        for root in roots {
            if let Some(build) = storage.latest_build(&bucket, root, job).await? {
                return Ok(Some(build));
            }
        }
        Ok(None)
    }

    fn find_build<'a>(jobs: &'a [ProwJob], job: &str, run_number: i64) -> Option<&'a ProwJob> {
        jobs.iter().find(|candidate| {
            candidate.spec.job == job && candidate.run_number() == Some(run_number)
        })
    }

    fn target(&self, run: &RunRef<'_>) -> PluginResult<RunTarget> {
        let selection = config::parse_pipeline_id(run.pipeline_id)?;
        let deck = self.deck()?;
        let text = |key: &str| {
            run.metadata
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
        };
        let state = text("state").map(str::to_string);
        let artifacts = text("artifacts").and_then(Artifacts::from_metadata);
        let status = match (state.as_deref(), text("result")) {
            (Some(state), _) => mapper::map_state(state),
            (None, Some(result)) => mapper::map_result(result, None).0,
            (None, None) => PipelineStatus::Pending,
        };
        let build_id = Some(run.run_id)
            .filter(|id| !id.is_empty())
            .or_else(|| text("build_id"))
            .ok_or_else(|| {
                PluginError::PipelineNotFound("This run has no Prow build ID".to_string())
            })?
            .to_string();
        Ok(RunTarget {
            job: selection.job,
            build_id,
            state,
            web_url: artifacts
                .as_ref()
                .map(|artifacts| artifacts.view_url(deck.base())),
            artifacts,
            status,
        })
    }

    async fn history_from(
        &self, jobs: &[ProwJob], selection: &Selection, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let deck = self.deck()?;
        let mut runs = mapper::runs(jobs, &selection.job, pipeline_id, deck.base());
        if self.history == HistorySource::Storage {
            if let Some((bucket, roots)) =
                self.history_location(mapper::newest(jobs, &selection.job))
            {
                let stored = self
                    .stored_runs(&bucket, &roots, selection, pipeline_id, &runs, limit)
                    .await?;
                runs.extend(stored);
            }
        }
        runs.sort_by_key(|run| std::cmp::Reverse(run.run_number));
        runs.truncate(limit);
        Ok(runs)
    }

    async fn stream_from(
        &self, source: LogSource, target: &RunTarget, offset: Option<u64>, sink: &mut LogSink,
    ) -> PluginResult<(LogReadMode, u64)> {
        let deck = self.deck()?;
        match (source, &target.artifacts) {
            (LogSource::Storage, Some(artifacts)) => {
                self.storage()?
                    .stream_build_log(artifacts, offset, sink)
                    .await
            }
            _ => {
                deck.stream_pod_log(&target.job, &target.build_id, offset, sink)
                    .await
            }
        }
    }

    async fn stored_runs(
        &self, bucket: &Bucket, roots: &[HistoryRoot], selection: &Selection, pipeline_id: &str,
        deck_runs: &[PipelineRun], limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let storage = self.storage()?;
        let deck = self.deck()?;
        let floor = deck_runs
            .get(limit.saturating_sub(1))
            .filter(|_| limit > 0)
            .map_or(i64::MIN, |run| run.run_number);
        let now = Utc::now().timestamp_millis();

        for root in roots {
            let mut seen: HashSet<i64> = deck_runs.iter().map(|run| run.run_number).collect();
            let mut stored: Vec<PipelineRun> = Vec::new();
            let mut listed_any = false;
            let mut span = HISTORY_WINDOW_MS;
            loop {
                let from = storage::listing_floor(now.saturating_sub(span));
                let numbers = storage
                    .build_numbers(bucket, *root, &selection.job, from, None)
                    .await?;
                listed_any |= !numbers.is_empty();
                let candidates: Vec<i64> = numbers
                    .into_iter()
                    .filter(|build_id| {
                        let run_number = run_number_of(*build_id);
                        run_number > floor && seen.insert(run_number)
                    })
                    .collect();
                for chunk in candidates.chunks(limit.max(1)) {
                    let builds = storage
                        .builds(bucket, *root, &selection.job, chunk.to_vec())
                        .await?;
                    stored.extend(builds.iter().map(|build| {
                        mapper::map_stored_run(
                            build,
                            pipeline_id,
                            selection.scope.as_ref(),
                            deck.base(),
                        )
                    }));
                    if stored.len() >= limit {
                        return Ok(stored);
                    }
                }
                if from.is_none_or(|from| run_number_of(from) <= floor) {
                    break;
                }
                span = span.saturating_mul(HISTORY_WINDOW_GROWTH);
            }
            if listed_any {
                return Ok(stored);
            }
        }
        Ok(Vec::new())
    }
}

#[async_trait]
impl Plugin for ProwPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.metadata
    }

    fn provider_type(&self) -> &str {
        config::PROVIDER_TYPE
    }

    fn client_options(&self, config: &HashMap<String, String>) -> ClientOptions {
        ClientOptions {
            origin: config::deck_url(config).ok(),
            accept_invalid_certs: config::is_insecure(config),
        }
    }

    fn status_source(&self) -> Option<&StatusSource> {
        self.status_source.as_ref()
    }

    fn initialize(
        &mut self, provider_id: i64, config: HashMap<String, String>,
        http_client: Option<Arc<reqwest::Client>>,
    ) -> PluginResult<()> {
        info!(provider_id, "Initializing Prow plugin");

        let status_source = custom_status_source(&config)?;
        let base = config::deck_url(&config)?;
        let read_only = is_no_auth(&config);
        let auth = if read_only {
            None
        } else {
            config::auth_header(&config)?
        };
        let history = config::history_source(&config)?;
        let storage_url = config::storage_url(&config)?;
        let storage_bucket = config::storage_bucket(&config)?;
        let selections = config::selections(&config)?;
        let kube_settings = config::kube_settings(&config).filter(|_| !read_only);
        if config::is_insecure(&config) {
            warn!("Insecure TLS mode enabled - certificate verification disabled");
        }
        let http_client = http_client.ok_or_else(|| {
            PluginError::Internal("Prow plugin requires an HTTP client".to_string())
        })?;

        let deck = Arc::new(DeckClient::new(Arc::clone(&http_client), base, auth));
        self.storage = Some(StorageClient::new(
            http_client,
            Arc::clone(&deck),
            storage_url,
        ));
        self.deck = Some(deck);
        self.provider_id = Some(provider_id);
        self.history = history;
        self.storage_bucket = storage_bucket;
        self.selections = selections;
        self.read_only = read_only;
        self.kube_settings = kube_settings;
        self.kube = OnceCell::new();
        self.status_source = status_source;
        Ok(())
    }

    async fn validate_credentials(&self) -> PluginResult<bool> {
        self.deck()?.check_access().await?;
        debug!("Prow Deck reachable");
        Ok(true)
    }

    async fn fetch_organizations(&self) -> PluginResult<Vec<Organization>> {
        Ok(self.fetch_pipeline_catalog().await?.organizations)
    }

    async fn fetch_pipeline_catalog(&self) -> PluginResult<PipelineCatalog> {
        let deck = self.deck()?;
        let jobs = deck.jobs(None, None).await?;
        Ok(mapper::catalog(&jobs, deck.host()))
    }

    async fn fetch_available_pipelines(
        &self, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        self.fetch_available_pipelines_filtered(None, None, params)
            .await
    }

    async fn fetch_available_pipelines_filtered(
        &self, org: Option<String>, search: Option<String>, params: Option<PaginationParams>,
    ) -> PluginResult<PaginatedResponse<AvailablePipeline>> {
        let deck = self.deck()?;
        let jobs = match org.as_deref() {
            Some(org) if org != deck.host() => deck.jobs(Some(org), None).await?,
            _ => deck.jobs(None, None).await?,
        };
        let search = search.map(|term| term.to_lowercase());
        let matching: Vec<AvailablePipeline> = mapper::catalog(&jobs, deck.host())
            .pipelines
            .into_iter()
            .filter(|pipeline| org.is_none() || pipeline.organization == org)
            .filter(|pipeline| {
                search.as_ref().is_none_or(|term| {
                    pipeline.name.to_lowercase().contains(term)
                        || pipeline
                            .repository
                            .as_ref()
                            .is_some_and(|repository| repository.to_lowercase().contains(term))
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
        if self.selections.is_empty() {
            return Ok(Vec::new());
        }
        let deck = self.deck()?;

        let mut scopes: Vec<Option<&Scope>> = Vec::new();
        for selection in &self.selections {
            let scope = selection.scope.as_ref();
            if !scopes.contains(&scope) {
                scopes.push(scope);
            }
        }
        let lists =
            futures::future::try_join_all(scopes.iter().map(|scope| self.scope_jobs(*scope)))
                .await?;
        let jobs_by_scope: HashMap<Option<&Scope>, Vec<ProwJob>> =
            scopes.into_iter().zip(lists).collect();
        let newest: Vec<Option<&ProwJob>> = self
            .selections
            .iter()
            .map(|selection| {
                jobs_by_scope
                    .get(&selection.scope.as_ref())
                    .and_then(|jobs| mapper::newest(jobs, &selection.job))
            })
            .collect();

        let stored: Vec<Option<StoredBuild>> = if self.history == HistorySource::Storage {
            futures::future::try_join_all(self.selections.iter().zip(&newest).map(
                |(selection, newest)| async move {
                    match newest {
                        Some(_) => Ok(None),
                        None => self.stored_latest(&selection.job).await,
                    }
                },
            ))
            .await?
        } else {
            vec![None; self.selections.len()]
        };

        let pipelines: Vec<Pipeline> = self
            .selections
            .iter()
            .zip(newest)
            .zip(&stored)
            .map(|((selection, newest), stored)| {
                let latest = match (newest, stored) {
                    (Some(job), _) => Latest::Deck(job),
                    (None, Some(build)) => Latest::Storage(build),
                    (None, None) => Latest::None,
                };
                mapper::map_pipeline(provider_id, selection, latest, deck.base(), self.read_only)
            })
            .collect();

        info!(count = pipelines.len(), "Mapped Prow jobs");
        Ok(pipelines)
    }

    async fn fetch_run_history(
        &self, pipeline_id: &str, limit: usize,
    ) -> PluginResult<Vec<PipelineRun>> {
        let selection = config::parse_pipeline_id(pipeline_id)?;
        let jobs = self.scope_jobs(selection.scope.as_ref()).await?;
        self.history_from(&jobs, &selection, pipeline_id, limit)
            .await
    }

    async fn fetch_recent_runs(
        &self, pipeline_ids: &[String], limit: usize,
    ) -> PluginResult<HashMap<String, Vec<PipelineRun>>> {
        let selections: Vec<(&String, Selection)> = pipeline_ids
            .iter()
            .filter_map(|id| {
                config::parse_pipeline_id(id)
                    .ok()
                    .map(|selection| (id, selection))
            })
            .collect();
        let mut scopes: Vec<Option<&Scope>> = Vec::new();
        for (_, selection) in &selections {
            let scope = selection.scope.as_ref();
            if !scopes.contains(&scope) {
                scopes.push(scope);
            }
        }
        let lists =
            futures::future::try_join_all(scopes.iter().map(|scope| self.scope_jobs(*scope)))
                .await?;
        let jobs_by_scope: HashMap<Option<&Scope>, Vec<ProwJob>> =
            scopes.into_iter().zip(lists).collect();

        let histories: Vec<_> = selections
            .iter()
            .map(|(id, selection)| {
                let jobs = jobs_by_scope
                    .get(&selection.scope.as_ref())
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                async move {
                    let runs = self.history_from(jobs, selection, id, limit).await?;
                    Ok::<_, PluginError>(((*id).clone(), runs))
                }
            })
            .collect();
        futures::stream::iter(histories)
            .buffered(HISTORY_CONCURRENCY)
            .try_collect()
            .await
    }

    async fn fetch_run_details(
        &self, pipeline_id: &str, run_number: i64,
    ) -> PluginResult<PipelineRun> {
        let selection = config::parse_pipeline_id(pipeline_id)?;
        let deck = self.deck()?;
        let jobs = self.scope_jobs(selection.scope.as_ref()).await?;

        if let Some(job) = Self::find_build(&jobs, &selection.job, run_number) {
            return mapper::map_run(job, pipeline_id, deck.base()).ok_or_else(|| {
                PluginError::PipelineNotFound(format!(
                    "Build {} of {} has not started yet",
                    job.status.build_id, selection.job
                ))
            });
        }

        if self.history == HistorySource::Storage {
            if let Some((bucket, roots)) =
                self.history_location(mapper::newest(&jobs, &selection.job))
            {
                let storage = self.storage()?;
                for root in roots {
                    if let Some(build) = storage
                        .find_build(&bucket, root, &selection.job, run_number)
                        .await?
                    {
                        return Ok(mapper::map_stored_run(
                            &build,
                            pipeline_id,
                            selection.scope.as_ref(),
                            deck.base(),
                        ));
                    }
                }
            }
        }

        Err(PluginError::PipelineNotFound(format!(
            "This build of {} is no longer listed by Deck",
            selection.job
        )))
    }

    async fn fetch_workflow_parameters(
        &self, workflow_id: &str,
    ) -> PluginResult<Vec<WorkflowParameter>> {
        config::parse_pipeline_id(workflow_id)?;
        let mut parameters = vec![WorkflowParameter {
            name: "prowjob".to_string(),
            label: Some("Run to start again".to_string()),
            description: Some(
                "Name of the ProwJob to start again. Leave empty to use the newest run."
                    .to_string(),
            ),
            param_type: WorkflowParameterType::String { default: None },
            required: false,
        }];
        if self.kube_settings.is_none() {
            parameters.push(WorkflowParameter {
                name: "definition".to_string(),
                label: Some("Job definition".to_string()),
                description: Some(
                    "latest rebuilds the job from Prow's current configuration. original reuses \
                     the exact spec of that run."
                        .to_string(),
                ),
                param_type: WorkflowParameterType::Choice {
                    options: vec!["latest".to_string(), "original".to_string()],
                    default: Some("latest".to_string()),
                },
                required: true,
            });
        }
        Ok(parameters)
    }

    async fn trigger_pipeline(&self, params: TriggerParams) -> PluginResult<String> {
        self.writable()?;
        let selection = config::parse_pipeline_id(&params.workflow_id)?;
        let job = selection.job.as_str();
        let input = |key: &str| {
            params
                .inputs
                .as_ref()
                .and_then(|inputs| inputs.get(key))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        };
        let deck = self.deck()?;
        let jobs = self.scope_jobs(selection.scope.as_ref()).await?;
        let target = match input("prowjob") {
            Some(requested) => jobs
                .iter()
                .find(|candidate| candidate.metadata.name == requested && candidate.spec.job == job)
                .ok_or_else(|| {
                    PluginError::PipelineNotFound(format!(
                        "ProwJob {requested} is not a recent run of {job}"
                    ))
                })?,
            None => mapper::newest(&jobs, job).ok_or_else(|| {
                PluginError::PipelineNotFound(format!(
                    "Deck lists no recent run of {job}, and Prow starts a job again only from an \
                     earlier run"
                ))
            })?,
        };
        let name = target.metadata.name.as_str();

        let (message, created) = match self.kube().await? {
            Some(kube) => {
                let created = kube.start_again(target.namespace(), name).await?;
                (
                    format!("Started {job} again as ProwJob {created}"),
                    Some(created),
                )
            }
            None => {
                let mode = if input("definition") == Some("original") {
                    RerunMode::Original
                } else {
                    RerunMode::Latest
                };
                deck.rerun(name, mode).await?;
                (
                    format!("Asked Deck to start {job} again from ProwJob {name}"),
                    None,
                )
            }
        };
        info!(job, "Prow job started again");

        Ok(json!({ "message": message, "prowjob": created }).to_string())
    }

    async fn cancel_run(&self, pipeline_id: &str, run_number: i64) -> PluginResult<()> {
        self.writable()?;
        let selection = config::parse_pipeline_id(pipeline_id)?;
        let job = selection.job.as_str();
        let deck = self.deck()?;
        let jobs = self.scope_jobs(selection.scope.as_ref()).await?;
        let target = Self::find_build(&jobs, job, run_number).ok_or_else(|| {
            PluginError::PipelineNotFound(format!(
                "This build of {job} is no longer listed by Deck"
            ))
        })?;
        if !target.is_active() {
            return Err(PluginError::ApiError(format!(
                "Build {} of {job} already finished ({})",
                target.status.build_id, target.status.state
            )));
        }

        match self.kube().await? {
            Some(kube) => {
                kube.abort(target.namespace(), &target.metadata.name)
                    .await?
            }
            None => deck.abort(&target.metadata.name).await?,
        }
        info!(job, run_number, "Prow job aborted");
        Ok(())
    }

    async fn fetch_log_nodes(&self, run: RunRef<'_>) -> PluginResult<Vec<LogNode>> {
        let target = self.target(&run)?;
        let availability = match target.state.as_deref() {
            Some("scheduling" | "triggered") => LogAvailability::Pending,
            Some("pending") => LogAvailability::Partial,
            _ => LogAvailability::Ready,
        };
        Ok(vec![mapper::log_node(
            target.status,
            availability,
            target.web_url,
        )])
    }

    async fn read_log(
        &self, run: RunRef<'_>, node_id: &str, from: Option<&LogCursor>, sink: &mut LogSink,
    ) -> PluginResult<LogRead> {
        if node_id != mapper::LOG_NODE {
            return Err(PluginError::InvalidConfig(format!(
                "Invalid Prow log node: {node_id}"
            )));
        }
        let target = self.target(&run)?;
        let state = target.state.as_deref();
        if matches!(state, Some("scheduling" | "triggered")) {
            return Err(PluginError::PipelineNotFound(format!(
                "Build {} has no log yet",
                target.build_id
            )));
        }
        let storage = self.storage()?;
        let stored = state != Some("pending")
            && target
                .artifacts
                .as_ref()
                .is_some_and(|artifacts| storage.log_url(artifacts).is_some());
        let previous = from.and_then(|cursor| cursor.parse::<LogPosition>());
        let source = match (stored, &previous) {
            (false, _) => LogSource::Deck,
            (true, Some(previous)) => previous.source,
            (true, None) => LogSource::Storage,
        };
        let offset = previous
            .filter(|previous| previous.source == source)
            .map(|previous| previous.offset);
        if from.is_some() && offset.is_none() {
            sink.restart()?;
        }

        let (source, (mode, offset)) = match self.stream_from(source, &target, offset, sink).await {
            Ok(read) => (source, read),
            Err(PluginError::PipelineNotFound(_)) if stored && sink.bytes_written() == 0 => {
                if from.is_some() && !sink.restarted() {
                    sink.restart()?;
                }
                let fallback = source.other();
                match self.stream_from(fallback, &target, None, sink).await {
                    Ok(read) => (fallback, read),
                    Err(PluginError::PipelineNotFound(_)) => {
                        return Err(PluginError::PipelineNotFound(format!(
                            "The log of build {} is no longer available",
                            target.build_id
                        )))
                    }
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error),
        };

        Ok(LogRead {
            mode,
            cursor: Some(LogCursor::new(&LogPosition { source, offset })?),
        })
    }

    async fn get_field_options(
        &self, field_key: &str, config: &HashMap<String, String>,
    ) -> PluginResult<Vec<String>> {
        if field_key != config::KUBE_CONTEXT {
            return Ok(Vec::new());
        }
        let Some(settings) = config::kube_settings(config) else {
            return Ok(Vec::new());
        };
        match ProwJobClient::contexts(settings.paths).await {
            Ok(contexts) => Ok(contexts),
            Err(error) => {
                warn!(%error, "Couldn't read kubeconfig contexts");
                Ok(Vec::new())
            }
        }
    }

    async fn sign_in_client(
        &self, config: &HashMap<String, String>, _http_client: &reqwest::Client,
    ) -> PluginResult<SignInClient> {
        sign_in::client_for(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{
        gcs,
        serve,
        Request,
        Requests,
        Response,
    };

    const STARTED: &str = "2026-10-30T10:00:00Z";
    const FINISHED: &str = "2026-10-30T10:05:00Z";
    const CI_A: &str = "prow__1__kubernetes/test-infra/ci-a";
    const PULL_X: &str = "prow__1__kubernetes/test-infra/pull-x";

    fn prowjob(
        name: &str, job: &str, build_id: &str, state: &str, job_type: &str, url: &str,
    ) -> String {
        let completion = if matches!(state, "success" | "failure" | "aborted" | "error") {
            format!(r#","completionTime":"{FINISHED}""#)
        } else {
            String::new()
        };
        format!(
            r#"{{"metadata":{{"name":"{name}","namespace":"default","creationTimestamp":"{STARTED}"}},"spec":{{"type":"{job_type}","cluster":"default","job":"{job}","refs":{{"org":"kubernetes","repo":"test-infra","base_ref":"main","base_sha":"abc123"}}}},"status":{{"startTime":"{STARTED}"{completion},"state":"{state}","description":"Job {state}","url":"{url}","build_id":"{build_id}"}}}}"#
        )
    }

    fn jobs_body(jobs: &[String]) -> String {
        format!(r#"{{"items":[{}]}}"#, jobs.join(","))
    }

    fn view(deck: &str, path: &str) -> String {
        format!("{deck}/view/gs/bucket/{path}")
    }

    fn plugin_with(deck: &str, extra: &[(&str, &str)]) -> ProwPlugin {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let mut config = HashMap::from([
            ("deck_url".to_string(), deck.to_string()),
            (
                "selected_items".to_string(),
                "kubernetes/test-infra/ci-a".to_string(),
            ),
        ]);
        for (key, value) in extra {
            config.insert(key.to_string(), value.to_string());
        }
        let mut plugin = ProwPlugin::new();
        plugin
            .initialize(1, config, Some(Arc::new(reqwest::Client::new())))
            .unwrap();
        plugin
    }

    fn sent(requests: &Requests, method: &str, path: &str) -> Vec<Request> {
        requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.method == method && request.target.starts_with(path))
            .cloned()
            .collect()
    }

    fn deck_serving(
        jobs: impl Fn(&str) -> Vec<String> + Send + Sync + 'static,
        other: impl Fn(&Request) -> Response + Send + Sync + 'static,
    ) -> (String, Requests) {
        let deck = Arc::new(std::sync::OnceLock::<String>::new());
        let base = Arc::clone(&deck);
        let served = serve(move |request| {
            if request.target.starts_with("/prowjobs.js") {
                let base = base.get().map(String::as_str).unwrap_or_default();
                Response::json(200, &jobs_body(&jobs(base)))
            } else {
                other(request)
            }
        });
        deck.set(served.0.clone()).unwrap();
        served
    }

    async fn read(
        plugin: &ProwPlugin, pipeline_id: &str, run_number: i64, from: Option<&LogCursor>,
    ) -> PluginResult<(LogRead, String)> {
        let run = plugin.fetch_run_details(pipeline_id, run_number).await?;
        read_run(plugin, &run, from).await
    }

    fn run_ref(run: &PipelineRun) -> RunRef<'_> {
        RunRef {
            pipeline_id: &run.pipeline_id,
            run_number: run.run_number,
            run_id: &run.id,
            metadata: &run.metadata,
        }
    }

    async fn read_run(
        plugin: &ProwPlugin, run: &PipelineRun, from: Option<&LogCursor>,
    ) -> PluginResult<(LogRead, String)> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
        let mut sink = LogSink::new(tx);
        let result = plugin
            .read_log(run_ref(run), mapper::LOG_NODE, from, &mut sink)
            .await;
        drop(sink);
        let mut text = Vec::new();
        while let Some(chunk) = rx.recv().await {
            text.extend_from_slice(&chunk);
        }
        result.map(|read| (read, String::from_utf8(text).unwrap()))
    }

    fn source_of(read: &LogRead) -> LogSource {
        read.cursor
            .as_ref()
            .and_then(|cursor| cursor.parse::<LogPosition>())
            .unwrap()
            .source
    }

    #[tokio::test]
    async fn pipelines_fetch_each_scope_once_and_decode_gzip() {
        let body = jobs_body(&[
            prowjob("a-300", "ci-a", "300", "success", "periodic", ""),
            prowjob("b-301", "ci-b", "301", "pending", "periodic", ""),
            prowjob("a-299", "ci-a", "299", "failure", "periodic", ""),
        ]);
        let (deck, requests) = serve(move |_| Response::gzip_json(&body));
        let plugin = plugin_with(
            &deck,
            &[(
                "selected_items",
                "kubernetes/test-infra/ci-a, kubernetes/test-infra/ci-b",
            )],
        );

        let pipelines = plugin.fetch_pipelines().await.unwrap();

        let summary: Vec<(&str, PipelineStatus, &str)> = pipelines
            .iter()
            .map(|p| (p.name.as_str(), p.status, p.repository.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                ("ci-a", PipelineStatus::Success, "kubernetes/test-infra"),
                ("ci-b", PipelineStatus::Running, "kubernetes/test-infra")
            ]
        );
        assert_eq!(pipelines[0].id, CI_A);
        assert_eq!(pipelines[0].metadata["job_type"], "periodic");
        let lists = sent(&requests, "GET", "/prowjobs.js");
        assert_eq!(lists.len(), 1);
        assert!(lists[0].target.contains("org=kubernetes"));
        assert!(lists[0].target.contains("repo=test-infra"));
        assert!(lists[0]
            .target
            .contains("omit=annotations%2Clabels%2Cdecoration_config%2Cpod_spec"));
        assert_eq!(lists[0].header("accept-encoding").as_deref(), Some("gzip"));
    }

    #[tokio::test]
    async fn pipeline_without_recent_runs_shows_no_recent_runs() {
        let body = jobs_body(&[prowjob("b-1", "ci-b", "1", "success", "periodic", "")]);
        let (deck, _) = serve(move |_| Response::json(200, &body));
        let plugin = plugin_with(&deck, &[]);

        let pipelines = plugin.fetch_pipelines().await.unwrap();

        assert_eq!(pipelines.len(), 1);
        assert_eq!(pipelines[0].status, PipelineStatus::Pending);
        assert_eq!(pipelines[0].last_run, None);
        assert_eq!(
            pipelines[0]
                .resource_state
                .as_ref()
                .map(|s| s.label.as_str()),
            Some("No recent runs")
        );
    }

    #[tokio::test]
    async fn recent_runs_fetch_each_scope_once_for_many_pipelines() {
        let refless = r#"{"metadata":{"name":"n-7","namespace":"default"},"spec":{"type":"periodic","job":"ci-noref"},"status":{"startTime":"2026-10-30T10:00:00Z","state":"pending","build_id":"7"}}"#;
        let scoped = jobs_body(&[
            prowjob("a-300", "ci-a", "300", "success", "periodic", ""),
            prowjob("b-301", "ci-b", "301", "pending", "periodic", ""),
            prowjob("a-299", "ci-a", "299", "failure", "periodic", ""),
        ]);
        let everything = format!(r#"{{"items":[{refless}]}}"#);
        let (deck, requests) = serve(move |request| {
            if request.target.contains("org=") {
                Response::json(200, &scoped)
            } else {
                Response::json(200, &everything)
            }
        });
        let plugin = plugin_with(&deck, &[]);
        let ids = [
            CI_A.to_string(),
            "prow__1__kubernetes/test-infra/ci-b".to_string(),
            "prow__1__ci-noref".to_string(),
            "not-a-prow-id".to_string(),
        ];

        let runs = plugin.fetch_recent_runs(&ids, 10).await.unwrap();

        let numbers =
            |id: &str| -> Vec<i64> { runs[id].iter().map(|run| run.run_number).collect() };
        assert_eq!(numbers(&ids[0]), [300, 299]);
        assert_eq!(numbers(&ids[1]), [301]);
        assert_eq!(numbers(&ids[2]), [7]);
        assert!(!runs.contains_key(&ids[3]));
        assert_eq!(sent(&requests, "GET", "/prowjobs.js").len(), 2);
    }

    #[tokio::test]
    async fn recent_runs_fail_as_a_whole_when_one_scope_fails() {
        let body = jobs_body(&[prowjob("a-300", "ci-a", "300", "success", "periodic", "")]);
        let (deck, _) = serve(move |request| {
            if request.target.contains("repo=other") {
                Response::text(403, "forbidden")
            } else {
                Response::json(200, &body)
            }
        });
        let plugin = plugin_with(&deck, &[]);
        let ids = [
            CI_A.to_string(),
            "prow__1__kubernetes/other/ci-z".to_string(),
        ];

        let result = plugin.fetch_recent_runs(&ids, 10).await;

        assert!(matches!(result, Err(PluginError::PermissionDenied(_))));
    }

    #[tokio::test]
    async fn run_history_is_newest_first_skips_unbuilt_jobs_and_honors_limit() {
        let body = jobs_body(&[
            prowjob("a-100", "ci-a", "100", "success", "periodic", ""),
            prowjob("a-300", "ci-a", "300", "aborted", "periodic", ""),
            prowjob("a-200", "ci-a", "200", "failure", "periodic", ""),
            prowjob("a-new", "ci-a", "", "triggered", "periodic", ""),
            prowjob("b-400", "ci-b", "400", "success", "periodic", ""),
        ]);
        let (deck, _) = serve(move |_| Response::json(200, &body));
        let plugin = plugin_with(&deck, &[]);

        let runs = plugin.fetch_run_history(CI_A, 2).await.unwrap();
        let numbers: Vec<i64> = runs.iter().map(|run| run.run_number).collect();
        assert_eq!(numbers, [300, 200]);
        assert_eq!(runs[0].status, PipelineStatus::Cancelled);
        assert_eq!(runs[0].metadata["status_label"], "Aborted");
        assert_eq!(runs[0].inputs, Some(json!({ "prowjob": "a-300" })));
        assert_eq!(runs[0].duration_seconds, Some(300));
        assert_eq!(runs[0].logs_url, format!("{deck}/prowjob?prowjob=a-300"));

        let all = plugin.fetch_run_history(CI_A, 10).await.unwrap();
        let numbers: Vec<i64> = all.iter().map(|run| run.run_number).collect();
        assert_eq!(numbers, [300, 200, 100]);
    }

    #[tokio::test]
    async fn sign_in_page_maps_to_authentication_failed() {
        let (deck, _) = serve(|_| Response::html(200, "<html>Sign in with GitHub</html>"));
        let plugin = plugin_with(&deck, &[]);

        assert!(matches!(
            plugin.fetch_pipelines().await,
            Err(PluginError::AuthenticationFailed(_))
        ));
        assert!(matches!(
            plugin.validate_credentials().await,
            Err(PluginError::AuthenticationFailed(_))
        ));

        let (deck, _) = serve(|_| Response {
            status: 200,
            headers: vec![("Content-Type", "image/svg+xml".to_string())],
            body: b"<svg/>".to_vec(),
        });
        assert!(plugin_with(&deck, &[])
            .validate_credentials()
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn token_goes_to_deck_but_not_to_other_origins() {
        let (storage, storage_requests) = serve(|_| Response::text(200, "done\n"));
        let (deck, deck_requests) = deck_serving(
            |deck| {
                vec![prowjob(
                    "a-300",
                    "ci-a",
                    "300",
                    "success",
                    "periodic",
                    &view(deck, "logs/ci-a/300"),
                )]
            },
            |_| Response::text(404, "missing"),
        );
        let plugin = plugin_with(&deck, &[("token", "secret"), ("storage_url", &storage)]);

        let (_, text) = read(&plugin, CI_A, 300, None).await.unwrap();

        assert_eq!(text, "done\n");
        let deck_requests = deck_requests.lock().unwrap().clone();
        assert!(!deck_requests.is_empty());
        assert!(deck_requests
            .iter()
            .all(|request| request.header("authorization").as_deref() == Some("Bearer secret")));
        let storage_requests = storage_requests.lock().unwrap().clone();
        assert_eq!(storage_requests.len(), 1);
        assert_eq!(
            storage_requests[0].target,
            "/bucket/logs/ci-a/300/build-log.txt"
        );
        assert_eq!(storage_requests[0].header("authorization"), None);
    }

    fn action_deck(rerun: fn(&str) -> Response) -> (String, Requests) {
        deck_serving(
            |_| {
                vec![
                    prowjob("a-299", "ci-a", "299", "failure", "periodic", ""),
                    prowjob("a-300", "ci-a", "300", "pending", "periodic", ""),
                    prowjob("b-400", "ci-b", "400", "success", "periodic", ""),
                ]
            },
            move |request| {
                if request.method != "POST" {
                    return Response::text(404, "missing");
                }
                rerun(&request.target)
            },
        )
    }

    fn trigger(inputs: Option<Value>) -> TriggerParams {
        TriggerParams {
            workflow_id: CI_A.to_string(),
            inputs,
        }
    }

    #[tokio::test]
    async fn trigger_posts_rerun_for_newest_run_with_latest_mode() {
        let (deck, requests) = action_deck(|_| {
            Response::text(
                200,
                "Job successfully triggered. Wait 30 seconds and refresh the page for the job to show up.",
            )
        });
        let plugin = plugin_with(&deck, &[]);

        let result = plugin.trigger_pipeline(trigger(None)).await.unwrap();
        let result: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            result["message"],
            "Asked Deck to start ci-a again from ProwJob a-300"
        );
        plugin
            .trigger_pipeline(trigger(Some(
                json!({ "prowjob": "a-299", "definition": "original" }),
            )))
            .await
            .unwrap();

        let posts = sent(&requests, "POST", "/");
        let targets: Vec<&str> = posts.iter().map(|post| post.target.as_str()).collect();
        assert_eq!(
            targets,
            ["/rerun?prowjob=a-300&mode=latest", "/rerun?prowjob=a-299"]
        );
        assert!(posts
            .iter()
            .all(|post| post.header("content-length").as_deref() == Some("0")));
    }

    #[tokio::test]
    async fn trigger_rejects_prowjob_from_another_job() {
        let (deck, requests) = action_deck(|_| Response::text(200, "Job successfully triggered."));
        let plugin = plugin_with(&deck, &[]);

        let result = plugin
            .trigger_pipeline(trigger(Some(json!({ "prowjob": "b-400" }))))
            .await;

        assert!(matches!(result, Err(PluginError::PipelineNotFound(_))));
        assert!(sent(&requests, "POST", "/").is_empty());
    }

    #[tokio::test]
    async fn deck_refusals_explain_how_to_enable_actions() {
        let (deck, _) = action_deck(|target| {
            if target.contains("a-300") {
                Response::text(401, "Error retrieving GitHub login")
            } else {
                Response::text(
                    405,
                    "Direct rerun feature is not enabled. Enable with the '--rerun-creates-job' flag.",
                )
            }
        });
        let plugin = plugin_with(&deck, &[]);

        match plugin.trigger_pipeline(trigger(None)).await {
            Err(PluginError::PermissionDenied(message)) => {
                assert!(message.contains("kubeconfig"));
                assert!(message.contains("Error retrieving GitHub login"));
            }
            other => panic!("expected PermissionDenied, got {other:?}"),
        }
        match plugin
            .trigger_pipeline(trigger(Some(json!({ "prowjob": "a-299" }))))
            .await
        {
            Err(PluginError::NotSupported(message)) => {
                assert!(message.contains("--rerun-creates-job"));
            }
            other => panic!("expected NotSupported, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn cancel_aborts_running_build_and_refuses_finished_ones() {
        let (deck, requests) = action_deck(|_| Response::text(200, "Job successfully aborted."));
        let plugin = plugin_with(&deck, &[]);

        plugin.cancel_run(CI_A, 300).await.unwrap();
        let finished = plugin.cancel_run(CI_A, 299).await;

        assert!(matches!(finished, Err(PluginError::ApiError(_))));
        let posts: Vec<String> = sent(&requests, "POST", "/")
            .into_iter()
            .map(|request| request.target)
            .collect();
        assert_eq!(posts, ["/abort?prowjob=a-300"]);
    }

    #[tokio::test]
    async fn no_auth_reads_without_credentials_and_refuses_actions() {
        let (deck, requests) = action_deck(|_| Response::text(200, "Job successfully triggered."));
        let public = plugin_with(&deck, &[("auth_mode", "none"), ("token", "secret")]);
        let signed = plugin_with(&deck, &[("token", "secret")]);

        let pipelines = public.fetch_pipelines().await.unwrap();
        let started = public.trigger_pipeline(trigger(None)).await;
        let aborted = public.cancel_run(CI_A, 300).await;

        assert_eq!(pipelines[0].metadata["read_only"], json!(true));
        assert!(matches!(started, Err(PluginError::NotSupported(_))));
        assert!(matches!(aborted, Err(PluginError::NotSupported(_))));
        let sent_publicly = requests.lock().unwrap().clone();
        assert!(!sent_publicly.is_empty());
        assert!(sent_publicly
            .iter()
            .all(|request| request.method == "GET" && request.header("authorization").is_none()));

        let pipelines = signed.fetch_pipelines().await.unwrap();
        assert!(!pipelines[0].metadata.contains_key("read_only"));
    }

    #[tokio::test]
    async fn catalog_lists_each_job_once_with_its_organization_from_one_request() {
        let refless = r#"{"metadata":{"name":"n-7","namespace":"default"},"spec":{"type":"periodic","job":"ci-noref"},"status":{"startTime":"2026-10-30T10:00:00Z","state":"pending","build_id":"7"}}"#;
        let gerrit = r#"{"metadata":{"name":"g-1","namespace":"default"},"spec":{"type":"postsubmit","job":"post-g","refs":{"org":"https://gerrit.example.com","repo":"proj"}},"status":{"startTime":"2026-10-30T10:00:00Z","state":"success","build_id":"1"}}"#;
        let body = jobs_body(&[
            prowjob("a-300", "ci-a", "300", "success", "periodic", ""),
            prowjob("a-299", "ci-a", "299", "failure", "periodic", ""),
            prowjob("p-1", "pull-x", "1", "pending", "presubmit", ""),
            refless.to_string(),
            gerrit.to_string(),
        ]);
        let (deck, requests) = serve(move |_| Response::gzip_json(&body));
        let plugin = plugin_with(&deck, &[]);
        let host = reqwest::Url::parse(&deck)
            .unwrap()
            .host_str()
            .unwrap()
            .to_string();

        let catalog = plugin.fetch_pipeline_catalog().await.unwrap();

        let organizations: Vec<(&str, &str)> = catalog
            .organizations
            .iter()
            .map(|org| (org.id.as_str(), org.name.as_str()))
            .collect();
        assert_eq!(
            organizations,
            [
                (host.as_str(), host.as_str()),
                ("https://gerrit.example.com", "gerrit.example.com"),
                ("kubernetes", "kubernetes"),
            ]
        );
        let pipelines: Vec<(&str, Option<&str>)> = catalog
            .pipelines
            .iter()
            .map(|pipeline| (pipeline.id.as_str(), pipeline.organization.as_deref()))
            .collect();
        assert_eq!(
            pipelines,
            [
                ("kubernetes/test-infra/ci-a", Some("kubernetes")),
                ("ci-noref", Some(host.as_str())),
                (
                    "https://gerrit.example.com/proj/post-g",
                    Some("https://gerrit.example.com")
                ),
                ("kubernetes/test-infra/pull-x", Some("kubernetes")),
            ]
        );
        let lists = sent(&requests, "GET", "/prowjobs.js");
        assert_eq!(lists.len(), 1);
        assert!(!lists[0].target.contains("org="));
    }

    #[tokio::test]
    async fn running_build_reads_pod_log_from_deck() {
        let (deck, requests) = deck_serving(
            |deck| {
                vec![prowjob(
                    "a-300",
                    "ci-a",
                    "300",
                    "pending",
                    "periodic",
                    &view(deck, "logs/ci-a/300"),
                )]
            },
            |request| {
                if request.target == "/log?job=ci-a&id=300" {
                    Response::text(200, "still running\n")
                } else {
                    Response::text(404, "missing")
                }
            },
        );
        let plugin = plugin_with(&deck, &[]);
        let run = plugin.fetch_run_details(CI_A, 300).await.unwrap();

        let nodes = plugin.fetch_log_nodes(run_ref(&run)).await.unwrap();
        let (read, text) = read_run(&plugin, &run, None).await.unwrap();

        assert_eq!(nodes[0].log, Some(LogAvailability::Partial));
        assert_eq!(nodes[0].status, PipelineStatus::Running);
        assert_eq!(text, "still running\n");
        assert_eq!(read.mode, LogReadMode::Replace);
        assert_eq!(source_of(&read), LogSource::Deck);
        assert_eq!(sent(&requests, "GET", "/log").len(), 1);
        assert_eq!(sent(&requests, "GET", "/prowjobs.js").len(), 1);
    }

    #[tokio::test]
    async fn finished_build_appends_from_storage_with_range() {
        let (storage, storage_requests) = serve(|request| match request.header("range") {
            Some(range) if range == "bytes=9-" => {
                Response::text(206, "line two\n").header("Content-Range", "bytes 9-17/18")
            }
            Some(_) => Response::text(416, ""),
            None => Response::text(200, "line one\n"),
        });
        let (deck, _) = deck_serving(
            |deck| {
                vec![prowjob(
                    "a-300",
                    "ci-a",
                    "300",
                    "success",
                    "periodic",
                    &view(deck, "logs/ci-a/300"),
                )]
            },
            |_| Response::text(404, "missing"),
        );
        let plugin = plugin_with(&deck, &[("storage_url", &storage)]);

        let (first, text) = read(&plugin, CI_A, 300, None).await.unwrap();
        assert_eq!(text, "line one\n");
        assert_eq!(first.mode, LogReadMode::Replace);
        assert_eq!(source_of(&first), LogSource::Storage);

        let (second, text) = read(&plugin, CI_A, 300, first.cursor.as_ref())
            .await
            .unwrap();
        assert_eq!(text, "line two\n");
        assert_eq!(second.mode, LogReadMode::Append);
        assert_eq!(
            second
                .cursor
                .unwrap()
                .parse::<LogPosition>()
                .unwrap()
                .offset,
            18
        );
        let ranges: Vec<Option<String>> = storage_requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| request.header("range"))
            .collect();
        assert_eq!(ranges, [None, Some("bytes=9-".to_string())]);
    }

    #[tokio::test]
    async fn missing_storage_log_falls_back_to_deck_and_stays_there() {
        let (storage, storage_requests) = serve(|_| Response::text(404, "No such object"));
        let (deck, deck_requests) = deck_serving(
            |deck| {
                vec![prowjob(
                    "a-300",
                    "ci-a",
                    "300",
                    "success",
                    "periodic",
                    &view(deck, "logs/ci-a/300"),
                )]
            },
            |request| {
                if request.target.starts_with("/log?") {
                    Response::text(200, "from the pod\n")
                } else {
                    Response::text(404, "missing")
                }
            },
        );
        let plugin = plugin_with(&deck, &[("storage_url", &storage)]);
        let run = plugin.fetch_run_details(CI_A, 300).await.unwrap();

        let (first, text) = read_run(&plugin, &run, None).await.unwrap();
        assert_eq!(text, "from the pod\n");
        assert_eq!(first.mode, LogReadMode::Replace);
        assert_eq!(source_of(&first), LogSource::Deck);

        let (second, _) = read_run(&plugin, &run, first.cursor.as_ref())
            .await
            .unwrap();
        assert_eq!(source_of(&second), LogSource::Deck);
        assert_eq!(storage_requests.lock().unwrap().len(), 1);
        assert_eq!(sent(&deck_requests, "GET", "/log").len(), 2);
    }

    #[tokio::test]
    async fn storage_sign_in_page_is_an_authentication_error() {
        let (storage, _) = serve(|_| Response::html(200, "<html>Sign in</html>"));
        let (deck, _) = deck_serving(
            |deck| {
                vec![prowjob(
                    "a-300",
                    "ci-a",
                    "300",
                    "success",
                    "periodic",
                    &view(deck, "logs/ci-a/300"),
                )]
            },
            |_| Response::text(200, "from the pod\n"),
        );
        let plugin = plugin_with(&deck, &[("storage_url", &storage)]);

        let result = read(&plugin, CI_A, 300, None).await;

        assert!(matches!(result, Err(PluginError::AuthenticationFailed(_))));
    }

    fn object(name: &str, body: &str) -> (String, String) {
        (name.to_string(), body.to_string())
    }

    fn started(seconds: i64) -> String {
        format!(
            r#"{{"timestamp":{seconds},"repos":{{"kubernetes/test-infra":"main:abc"}},"repo-commit":"c{seconds}"}}"#
        )
    }

    fn finished(seconds: i64, result: &str) -> String {
        format!(r#"{{"timestamp":{seconds},"passed":false,"result":"{result}"}}"#)
    }

    fn stored_build(job: &str, build_id: i64, started_at: i64) -> Vec<(String, String)> {
        vec![
            object(
                &format!("logs/{job}/{build_id}/started.json"),
                &started(started_at),
            ),
            object(
                &format!("logs/{job}/{build_id}/finished.json"),
                &finished(started_at + 60, "SUCCESS"),
            ),
        ]
    }

    #[tokio::test]
    async fn storage_history_merges_older_builds_and_skips_unfinished() {
        let objects = vec![
            object("logs/ci-a/100/started.json", &started(1_000)),
            object("logs/ci-a/100/finished.json", &finished(1_060, "SUCCESS")),
            object("logs/ci-a/200/started.json", &started(2_000)),
            object("logs/ci-a/200/finished.json", &finished(2_090, "FAILURE")),
            object("logs/ci-a/250/started.json", &started(2_500)),
            object("logs/ci-a/300/build-log.txt", "done\n"),
            object("logs/ci-b/300/started.json", &started(3_000)),
            object("logs/ci-b/200/started.json", &started(2_000)),
            object("logs/ci-b/200/finished.json", &finished(2_030, "SUCCESS")),
            object("logs/ci-b/100/started.json", &started(1_000)),
            object("logs/ci-b/100/finished.json", &finished(1_030, "SUCCESS")),
            object(
                "pr-logs/directory/pull-x/400.txt",
                "gs://bucket/pr-logs/pull/kubernetes_test-infra/8/pull-x/400\n",
            ),
            object("pr-logs/directory/pull-x/latest-build.txt", "400"),
            object(
                "pr-logs/pull/kubernetes_test-infra/8/pull-x/400/started.json",
                &started(4_000),
            ),
            object(
                "pr-logs/pull/kubernetes_test-infra/8/pull-x/400/finished.json",
                &finished(4_010, "ABORTED"),
            ),
        ];
        let (storage, _) = serve(gcs(objects));
        let (deck, _) = deck_serving(
            |deck| {
                vec![
                    prowjob(
                        "a-300",
                        "ci-a",
                        "300",
                        "success",
                        "periodic",
                        &view(deck, "logs/ci-a/300"),
                    ),
                    prowjob(
                        "x-500",
                        "pull-x",
                        "500",
                        "success",
                        "presubmit",
                        &view(deck, "pr-logs/pull/kubernetes_test-infra/9/pull-x/500"),
                    ),
                ]
            },
            |_| Response::text(404, "missing"),
        );
        let plugin = plugin_with(
            &deck,
            &[
                ("history_source", "storage"),
                ("storage_url", &storage),
                ("storage_bucket", "gs://bucket"),
            ],
        );

        let runs = plugin.fetch_run_history(CI_A, 10).await.unwrap();
        let numbers: Vec<i64> = runs.iter().map(|run| run.run_number).collect();
        assert_eq!(numbers, [300, 200, 100]);
        let older = &runs[1];
        assert_eq!(older.status, PipelineStatus::Failed);
        assert_eq!(older.commit_sha.as_deref(), Some("c2000"));
        assert_eq!(older.branch.as_deref(), Some("main"));
        assert_eq!(older.duration_seconds, Some(90));
        assert_eq!(older.logs_url, view(&deck, "logs/ci-a/200"));

        let only_storage = plugin
            .fetch_run_history("prow__1__kubernetes/test-infra/ci-b", 2)
            .await
            .unwrap();
        let numbers: Vec<i64> = only_storage.iter().map(|run| run.run_number).collect();
        assert_eq!(numbers, [200, 100]);

        let pulls = plugin.fetch_run_history(PULL_X, 10).await.unwrap();
        let numbers: Vec<i64> = pulls.iter().map(|run| run.run_number).collect();
        assert_eq!(numbers, [500, 400]);
        assert_eq!(pulls[1].status, PipelineStatus::Cancelled);
        assert_eq!(
            pulls[1].metadata["artifacts"],
            "gs://bucket/pr-logs/pull/kubernetes_test-infra/8/pull-x/400"
        );

        let details = plugin.fetch_run_details(CI_A, 100).await.unwrap();
        assert_eq!(details.status, PipelineStatus::Success);
    }

    #[tokio::test]
    async fn storage_history_authenticates_only_against_the_deck_origin() {
        let objects = stored_build("ci-a", 200, 2_000);
        let listing = gcs(objects.clone());
        let (deck, _) = serve(move |request| {
            if request.header("authorization").as_deref() != Some("Bearer secret") {
                return Response::text(401, "unauthorized");
            }
            if request.target.starts_with("/prowjobs.js") {
                let job = prowjob("a-300", "ci-a", "300", "success", "periodic", "");
                Response::json(200, &jobs_body(&[job]))
            } else {
                listing(request)
            }
        });
        let (other, other_requests) = serve(gcs(objects));
        let history = |storage: &str| {
            plugin_with(
                &deck,
                &[
                    ("token", "secret"),
                    ("history_source", "storage"),
                    ("storage_url", storage),
                    ("storage_bucket", "gs://bucket"),
                ],
            )
        };

        let same_origin = history(&deck).fetch_run_history(CI_A, 10).await.unwrap();
        let numbers: Vec<i64> = same_origin.iter().map(|run| run.run_number).collect();
        assert_eq!(numbers, [300, 200]);

        let cross_origin = history(&other).fetch_run_history(CI_A, 10).await.unwrap();
        let numbers: Vec<i64> = cross_origin.iter().map(|run| run.run_number).collect();
        assert_eq!(numbers, [300, 200]);
        let other_requests = other_requests.lock().unwrap().clone();
        assert!(!other_requests.is_empty());
        assert!(other_requests
            .iter()
            .all(|request| request.header("authorization").is_none()));
    }

    #[tokio::test]
    async fn storage_history_stops_widening_once_deck_fills_the_page() {
        let now = Utc::now().timestamp_millis();
        let snowflake = |hours: i64| {
            crate::types::snowflake_at(now - hours * 60 * 60 * 1000)
                .unwrap()
                .to_string()
        };
        let (recent, older) = (snowflake(1), snowflake(2));
        let mut objects = stored_build("ci-a", snowflake(10 * 24).parse().unwrap(), 1_000);
        objects.extend(stored_build(
            "ci-a",
            snowflake(400 * 24).parse().unwrap(),
            900,
        ));
        let (storage, storage_requests) = serve(gcs(objects));
        let (deck, _) = deck_serving(
            move |deck| {
                [&recent, &older]
                    .iter()
                    .map(|id| {
                        let url = view(deck, &format!("logs/ci-a/{id}"));
                        prowjob(&format!("a-{id}"), "ci-a", id, "success", "periodic", &url)
                    })
                    .collect()
            },
            |_| Response::text(404, "missing"),
        );
        let plugin = plugin_with(
            &deck,
            &[
                ("history_source", "storage"),
                ("storage_url", &storage),
                ("storage_bucket", "gs://bucket"),
            ],
        );

        let runs = plugin.fetch_run_history(CI_A, 2).await.unwrap();

        assert_eq!(runs.len(), 2);
        assert_eq!(
            sent(&storage_requests, "GET", "/storage/v1/b/bucket/o").len(),
            1
        );
    }

    #[tokio::test]
    async fn builds_from_before_2018_list_and_open() {
        let old = crate::types::snowflake_at(1_464_739_200_000).unwrap();
        let new = crate::types::snowflake_at(Utc::now().timestamp_millis() - 60_000).unwrap();
        let mut objects = stored_build("ci-o", old, 1_464_739_200);
        objects.extend(stored_build("ci-o", new, 1_700_000_000));
        let (storage, _) = serve(gcs(objects));
        let (deck, _) = deck_serving(|_| Vec::new(), |_| Response::text(404, "missing"));
        let plugin = plugin_with(
            &deck,
            &[
                ("history_source", "storage"),
                ("storage_url", &storage),
                ("storage_bucket", "gs://bucket"),
            ],
        );
        let pipeline_id = "prow__1__kubernetes/test-infra/ci-o";

        let runs = plugin.fetch_run_history(pipeline_id, 5).await.unwrap();
        let ids: Vec<String> = runs.iter().map(|run| run.id.clone()).collect();
        assert_eq!(ids, [new.to_string(), old.to_string()]);

        for run in &runs {
            let details = plugin
                .fetch_run_details(pipeline_id, run.run_number)
                .await
                .unwrap();
            assert_eq!(details.id, run.id);
        }
    }

    #[tokio::test]
    async fn storage_history_lists_recent_windows_and_widens_until_it_has_enough() {
        let now = Utc::now().timestamp_millis();
        let hours_ago = |hours: i64| now - hours * 60 * 60 * 1000;
        let builds: Vec<(i64, i64)> = [1, 3 * 24, 10 * 24, 400 * 24]
            .into_iter()
            .map(|hours| {
                let at = hours_ago(hours);
                (crate::types::snowflake_at(at).unwrap(), at / 1000)
            })
            .collect();
        let objects: Vec<(String, String)> = builds
            .iter()
            .flat_map(|(build_id, started_at)| stored_build("ci-w", *build_id, *started_at))
            .collect();
        let (storage, storage_requests) = serve(gcs(objects));
        let (deck, _) = deck_serving(|_| Vec::new(), |_| Response::text(404, "missing"));
        let plugin = plugin_with(
            &deck,
            &[
                ("history_source", "storage"),
                ("storage_url", &storage),
                ("storage_bucket", "gs://bucket"),
            ],
        );

        let runs = plugin
            .fetch_run_history("prow__1__kubernetes/test-infra/ci-w", 3)
            .await
            .unwrap();

        let ids: Vec<String> = runs.iter().map(|run| run.id.clone()).collect();
        let expected: Vec<String> = builds[..3].iter().map(|(id, _)| id.to_string()).collect();
        assert_eq!(ids, expected);
        let listings = sent(&storage_requests, "GET", "/storage/v1/b/bucket/o");
        assert!(listings
            .iter()
            .all(|listing| listing.target.contains("startOffset=")));
        assert!(storage_requests
            .lock()
            .unwrap()
            .iter()
            .all(|request| !request.target.contains(&builds[3].0.to_string())));
    }

    #[tokio::test]
    async fn snowflake_build_ids_round_trip_through_safe_run_numbers() {
        const NEWER: &str = "2116011604377079808";
        const OLDER: &str = "2116011600182775808";
        let (storage, storage_requests) = serve(|request| {
            let target = request.target.as_str();
            if target.starts_with("/storage/v1/b/bucket/o?") {
                return Response::json(
                    200,
                    &format!(r#"{{"prefixes":["logs/ci-a/{NEWER}/","logs/ci-a/{OLDER}/"]}}"#),
                );
            }
            if target == format!("/bucket/logs/ci-a/{OLDER}/started.json") {
                return Response::json(200, r#"{"timestamp":1000}"#);
            }
            if target == format!("/bucket/logs/ci-a/{OLDER}/finished.json") {
                return Response::json(200, r#"{"timestamp":1030,"result":"SUCCESS"}"#);
            }
            Response::text(404, "No such object")
        });
        let (deck, _) = deck_serving(
            |deck| {
                vec![prowjob(
                    "a-new",
                    "ci-a",
                    NEWER,
                    "success",
                    "periodic",
                    &view(deck, &format!("logs/ci-a/{NEWER}")),
                )]
            },
            |_| Response::text(404, "missing"),
        );
        let plugin = plugin_with(
            &deck,
            &[("history_source", "storage"), ("storage_url", &storage)],
        );

        let runs = plugin.fetch_run_history(CI_A, 10).await.unwrap();

        let ids: Vec<&str> = runs.iter().map(|run| run.id.as_str()).collect();
        assert_eq!(ids, [NEWER, OLDER]);
        for run in &runs {
            assert!(run.run_number < 1 << 53);
            assert_eq!(run.metadata["build_id"], run.id);
            let details = plugin
                .fetch_run_details(CI_A, run.run_number)
                .await
                .unwrap();
            assert_eq!(details.id, run.id);
        }
        assert!(storage_requests
            .lock()
            .unwrap()
            .iter()
            .any(|request| request.target == format!("/bucket/logs/ci-a/{OLDER}/started.json")));
    }

    #[tokio::test]
    async fn sequential_build_ids_above_u32_resolve_their_own_build() {
        const BUILD: i64 = 4_294_967_296;
        let (storage, storage_requests) = serve(|request| {
            let target = request.target.as_str();
            if target.starts_with("/storage/v1/b/bucket/o?") && target.contains("delimiter") {
                return Response::json(200, &format!(r#"{{"prefixes":["logs/ci-a/{BUILD}/"]}}"#));
            }
            if target == format!("/bucket/logs/ci-a/{BUILD}/started.json") {
                return Response::json(200, r#"{"timestamp":1000}"#);
            }
            if target == format!("/bucket/logs/ci-a/{BUILD}/finished.json") {
                return Response::json(200, r#"{"timestamp":1030,"result":"FAILURE"}"#);
            }
            Response::text(404, "No such object")
        });
        let (deck, deck_requests) = deck_serving(
            |_| Vec::new(),
            |request| {
                if request.target.starts_with("/log?") {
                    Response::text(200, "from the pod\n")
                } else {
                    Response::text(404, "missing")
                }
            },
        );
        let plugin = plugin_with(
            &deck,
            &[
                ("history_source", "storage"),
                ("storage_url", &storage),
                ("storage_bucket", "gs://bucket"),
            ],
        );

        let runs = plugin.fetch_run_history(CI_A, 10).await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_number, BUILD);
        let details = plugin.fetch_run_details(CI_A, BUILD).await.unwrap();
        assert_eq!(details.id, BUILD.to_string());
        assert_eq!(details.status, PipelineStatus::Failed);

        let (_, text) = read_run(&plugin, &runs[0], None).await.unwrap();
        assert_eq!(text, "from the pod\n");
        let logs = sent(&deck_requests, "GET", "/log");
        assert_eq!(logs[0].target, format!("/log?job=ci-a&id={BUILD}"));
        assert!(storage_requests
            .lock()
            .unwrap()
            .iter()
            .filter(|request| request.target.ends_with("started.json"))
            .all(|request| request.target == format!("/bucket/logs/ci-a/{BUILD}/started.json")));
    }

    fn kube_plugin(deck: &str, apiserver: &str, dir: &tempfile::TempDir) -> ProwPlugin {
        let kubeconfig = dir.path().join("kubeconfig");
        std::fs::write(
            &kubeconfig,
            format!(
                "apiVersion: v1\nkind: Config\nclusters:\n- name: c\n  cluster:\n    server: {apiserver}\ncontexts:\n- name: ctx\n  context:\n    cluster: c\n    user: u\ncurrent-context: ctx\nusers:\n- name: u\n  user:\n    token: t\n"
            ),
        )
        .unwrap();
        plugin_with(deck, &[("kubeconfig_path", &kubeconfig.to_string_lossy())])
    }

    const PROWJOBS_PATH: &str = "/apis/prow.k8s.io/v1/namespaces/default/prowjobs";

    #[tokio::test]
    async fn kube_trigger_creates_prowjob_and_skips_deck() {
        let dir = tempfile::tempdir().unwrap();
        let (apiserver, kube_requests) = serve(|request| {
            match (request.method.as_str(), request.target.as_str()) {
                ("GET", target) if target.starts_with(&format!("{PROWJOBS_PATH}/a-300")) => {
                    Response::json(
                        200,
                        r#"{"apiVersion":"prow.k8s.io/v1","kind":"ProwJob","metadata":{"name":"a-300","namespace":"default","resourceVersion":"7","labels":{"prow.k8s.io/build-id":"300","prow.k8s.io/job":"ci-a"}},"spec":{"type":"periodic","job":"ci-a"},"status":{"state":"success","build_id":"300"}}"#,
                    )
                }
                ("POST", target) if target.starts_with(PROWJOBS_PATH) => Response::json(
                    201,
                    r#"{"apiVersion":"prow.k8s.io/v1","kind":"ProwJob","metadata":{"name":"pipedash-x7k2p","namespace":"default"}}"#,
                ),
                _ => Response::json(404, r#"{"kind":"Status","code":404,"message":"not found"}"#),
            }
        });
        let (deck, deck_requests) =
            action_deck(|_| Response::text(200, "Job successfully triggered."));
        let plugin = kube_plugin(&deck, &apiserver, &dir);

        let parameters = plugin.fetch_workflow_parameters(CI_A).await.unwrap();
        let result = plugin
            .trigger_pipeline(trigger(Some(json!({ "prowjob": "a-300" }))))
            .await
            .unwrap();

        let names: Vec<&str> = parameters.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["prowjob"]);
        let result: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(result["prowjob"], "pipedash-x7k2p");
        assert!(sent(&deck_requests, "POST", "/").is_empty());
        let created = sent(&kube_requests, "POST", PROWJOBS_PATH);
        assert_eq!(created.len(), 1);
        let body: Value = serde_json::from_slice(&created[0].body).unwrap();
        assert_eq!(body["metadata"]["generateName"], "pipedash-");
        assert_eq!(
            body["metadata"]["labels"],
            json!({ "prow.k8s.io/job": "ci-a" })
        );
        assert_eq!(body["spec"]["job"], "ci-a");
        assert_eq!(body["status"]["state"], "triggered");
    }

    fn running_prowjob(version: &str, state: &str) -> String {
        format!(
            r#"{{"metadata":{{"name":"a-300","namespace":"default","resourceVersion":"{version}"}},"status":{{"state":"{state}"}}}}"#
        )
    }

    #[tokio::test]
    async fn kube_cancel_merge_patches_status_aborted_at_the_read_version() {
        let dir = tempfile::tempdir().unwrap();
        let (apiserver, kube_requests) = serve(|request| match request.method.as_str() {
            "GET" => Response::json(200, &running_prowjob("7", "pending")),
            "PATCH" => Response::json(200, &running_prowjob("8", "aborted")),
            _ => Response::json(404, r#"{"kind":"Status","code":404,"message":"not found"}"#),
        });
        let (deck, deck_requests) =
            action_deck(|_| Response::text(200, "Job successfully aborted."));
        let plugin = kube_plugin(&deck, &apiserver, &dir);

        plugin.cancel_run(CI_A, 300).await.unwrap();

        assert!(sent(&deck_requests, "POST", "/").is_empty());
        let patches = sent(&kube_requests, "PATCH", PROWJOBS_PATH);
        assert_eq!(patches.len(), 1);
        assert!(patches[0]
            .target
            .starts_with(&format!("{PROWJOBS_PATH}/a-300?")));
        assert_eq!(
            patches[0].header("content-type").as_deref(),
            Some("application/merge-patch+json")
        );
        let body: Value = serde_json::from_slice(&patches[0].body).unwrap();
        assert_eq!(body["status"]["state"], "aborted");
        assert_eq!(body["metadata"]["resourceVersion"], "7");
    }

    #[tokio::test]
    async fn kube_cancel_rereads_after_a_conflict_and_keeps_a_finished_result() {
        let dir = tempfile::tempdir().unwrap();
        let reads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = Arc::clone(&reads);
        let (apiserver, kube_requests) = serve(move |request| match request.method.as_str() {
            "GET" => match counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) {
                0 => Response::json(200, &running_prowjob("7", "pending")),
                _ => Response::json(200, &running_prowjob("8", "success")),
            },
            "PATCH" => Response::json(
                409,
                r#"{"kind":"Status","apiVersion":"v1","status":"Failure","message":"the object has been modified","reason":"Conflict","code":409}"#,
            ),
            _ => Response::json(404, r#"{"kind":"Status","code":404,"message":"not found"}"#),
        });
        let (deck, _) = action_deck(|_| Response::text(200, "Job successfully aborted."));
        let plugin = kube_plugin(&deck, &apiserver, &dir);

        let result = plugin.cancel_run(CI_A, 300).await;

        assert!(
            matches!(result, Err(PluginError::ApiError(message)) if message.contains("already finished (success)"))
        );
        assert_eq!(reads.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(sent(&kube_requests, "PATCH", PROWJOBS_PATH).len(), 1);
    }

    #[tokio::test]
    async fn deck_sign_in_page_on_an_action_is_an_authentication_error() {
        let (deck, _) = action_deck(|_| Response::html(200, "<html>Sign in</html>"));
        let plugin = plugin_with(&deck, &[]);

        let result = plugin.cancel_run(CI_A, 300).await;

        assert!(matches!(result, Err(PluginError::AuthenticationFailed(_))));
    }
}
