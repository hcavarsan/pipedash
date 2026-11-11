use std::collections::HashSet;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    ArtifactObservation,
    ClusterId,
    ConsumedArtifact,
    Correlation,
    Digest,
    ImageRef,
    PathScope,
    Provenance,
    RepoId,
    SourceObservation,
    TargetLifetime,
    TargetObservation,
    UnitId,
};
use pipedash_plugin_api::{
    ChangedFile,
    CommitInfo,
    CommitSha,
};
use sha2::{
    Digest as _,
    Sha256,
};
use tokio::io::{
    AsyncReadExt,
    AsyncWriteExt,
};
use tokio::net::TcpListener;

use super::super::vocab::{
    commit_node_id,
    COMMIT,
    CONFIG_LINEAGE,
    ORIGIN_PROVIDER,
    REL_CONSUMED,
};
use super::super::{
    CorrelationDeps,
    CorrelationService,
};
use crate::application::services::provider_service::ProviderService;
use crate::application::test_support::{
    pipeline,
    stub_run,
};
use crate::application::test_support::{
    plugins,
    Harness,
    StubProvider,
};
use crate::domain::application::{
    MemberSource,
    Stage,
};
use crate::domain::PipelineStatus;
use crate::infrastructure::database::{
    ArtifactObservationRow,
    ArtifactRow,
    RunEvidenceRow,
    SourceObservationRow,
    SubjectIdentityRow,
    PIPELINE_LEVEL,
};
use crate::infrastructure::{
    ConfigState,
    HttpClientManager,
    PipedashConfig,
};

pub(super) struct Fixture {
    pub(super) h: Harness,
    pub(super) service: Arc<CorrelationService>,
}

impl Fixture {
    pub(super) async fn new() -> Self {
        Self::with_config(PipedashConfig::default()).await
    }

    pub(super) async fn with_config(config: PipedashConfig) -> Self {
        let h = Harness::new().await;
        let config_state = ConfigState::from_config(
            config,
            h._temp_dir.path().join("enrich.toml"),
            h.token_store.clone(),
            plugins::registry(),
        );
        let service = CorrelationService::new(CorrelationDeps {
            repository: Arc::clone(&h.repository),
            provider_service: Arc::clone(&h.provider_service),
            pipeline_service: Arc::clone(&h.pipeline_service),
            log_service: Arc::clone(&h.log_service),
            config_state,
            event_bus: h.bus.clone(),
            presenter: Arc::clone(&h.presenter),
            http: Arc::new(HttpClientManager::new().unwrap()),
            event_interval: Some(Duration::from_millis(10)),
        });
        Self { h, service }
    }

    pub(super) async fn stub(
        &self, build: impl FnOnce(StubProvider) -> StubProvider,
    ) -> (i64, Arc<StubProvider>) {
        self.h.add_stub(|id| build(StubProvider::new(id))).await
    }

    pub(super) async fn consume(
        &self, provider_id: i64, subject: &str, digest: &str, repository: &str,
        first_seen: DateTime<Utc>,
    ) {
        let mut conn = self.service.store.conn();
        conn.upsert_subject_identity(&SubjectIdentityRow {
            subject_id: subject.to_string(),
            provider_id,
            unit_id: Some(format!("{provider_id}:{subject}")),
            unit_parent: None,
            unit_depends_on_json: "[]".to_string(),
            app_ref_authority: None,
            app_ref_scope: None,
            app_ref_id: None,
            app_label: None,
        })
        .await
        .unwrap();
        let row = ArtifactObservationRow {
            provider_id,
            subject_id: subject.to_string(),
            run_number: PIPELINE_LEVEL,
            relation: REL_CONSUMED.to_string(),
            ordinal: 0,
            repository: repository.to_string(),
            digest: Some(digest.to_string()),
            tag: None,
            target_ordinal: None,
            origin: ORIGIN_PROVIDER.to_string(),
        };
        conn.replace_artifact_observations(subject, PIPELINE_LEVEL, ORIGIN_PROVIDER, &[row])
            .await
            .unwrap();
        self.artifact(digest, repository, first_seen, None).await;
    }

    pub(super) async fn artifact(
        &self, digest: &str, repository: &str, first_seen: DateTime<Utc>,
        label_source: Option<&str>,
    ) {
        self.service
            .store
            .conn()
            .upsert_artifact(&ArtifactRow {
                digest: digest.to_string(),
                repository: repository.to_string(),
                first_seen,
                label_source: label_source.map(str::to_string),
                label_revision: None,
                registry_checked_at: None,
                log_scanned_at: None,
            })
            .await
            .unwrap();
    }

    pub(super) async fn source(
        &self, provider_id: i64, subject: &str, run_number: i64, repo: &str, commit: Option<&str>,
    ) {
        let row = SourceObservationRow {
            provider_id,
            subject_id: subject.to_string(),
            run_number,
            ordinal: 0,
            repo: repo.to_string(),
            path_scopes_json: "[]".to_string(),
            commit_sha: commit.map(str::to_string),
        };
        self.service
            .store
            .conn()
            .replace_source_observations(subject, run_number, &[row])
            .await
            .unwrap();
    }

    pub(super) async fn concluded_run(
        &self, pipeline: &str, run_number: i64, concluded_at: DateTime<Utc>,
    ) {
        self.service
            .store
            .conn()
            .upsert_run_evidence(&RunEvidenceRow {
                pipeline_id: pipeline.to_string(),
                run_number,
                outcome_recorded: true,
                timing_recorded: true,
                attempt: true,
                started_at: concluded_at - chrono::Duration::minutes(5),
                concluded_at: Some(concluded_at),
                status: "success".to_string(),
            })
            .await
            .unwrap();
    }
}

pub(super) fn sha(letter: char) -> String {
    letter.to_string().repeat(40)
}

pub(super) fn digest(letter: char) -> String {
    format!("sha256:{}", letter.to_string().repeat(64))
}

pub(super) fn commit_info(letter: char) -> CommitInfo {
    CommitInfo {
        sha: CommitSha::parse(&sha(letter)).unwrap(),
        authored_at: Utc::now(),
        committed_at: Utc::now(),
        message: "change".to_string(),
        parents: Vec::new(),
    }
}

pub(super) fn digest_of(body: &[u8]) -> String {
    let hex: String = Sha256::digest(body)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("sha256:{hex}")
}

pub(super) async fn registry(routes: Vec<(String, Vec<u8>)>) -> String {
    registry_after(routes, None).await
}

pub(super) async fn registry_after(
    routes: Vec<(String, Vec<u8>)>, removing: Option<(Arc<ProviderService>, i64)>,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let routes = routes.clone();
            let removing = removing.clone();
            tokio::spawn(async move {
                if let Some((service, provider)) = removing {
                    let _ = service.remove_provider(provider).await;
                }
                let mut buffer = vec![0u8; 8192];
                let read = socket.read(&mut buffer).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                let path = request.split_whitespace().nth(1).unwrap_or_default();
                let reply = match routes.iter().find(|(known, _)| known == path) {
                    Some((_, body)) => {
                        let head = format!(
                            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                            body.len()
                        );
                        [head.into_bytes(), body.clone()].concat()
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                let _ = socket.write_all(&reply).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    address.to_string()
}

async fn seed_lineage(f: &Fixture) -> (String, String) {
    let running = digest('d');
    let (provider, _) = f
        .stub(|stub| {
            stub.with_source_hosts(&["git.test"])
                .with_changes(vec![ChangedFile {
                    path: "deploy.yaml".into(),
                    patch: Some(format!("+  image: registry.test/shop/api@{running}\n")),
                }])
        })
        .await;
    let subject = format!("stub__{provider}__api-prod");
    f.consume(
        provider,
        &subject,
        &running,
        "registry.test/shop/api",
        Utc::now(),
    )
    .await;
    f.source(
        provider,
        &subject,
        PIPELINE_LEVEL,
        "git.test/shop/config",
        Some(&sha('c')),
    )
    .await;
    (subject, commit_node_id("git.test/shop/config", &sha('c')))
}

async fn lineage_edges(f: &Fixture, node: &str) -> usize {
    f.service
        .store
        .conn()
        .list_edges_from(COMMIT, node)
        .await
        .unwrap()
        .iter()
        .filter(|edge| edge.rule == CONFIG_LINEAGE)
        .count()
}

#[tokio::test]
async fn a_pass_writes_evidence() {
    let f = Fixture::new().await;
    let (_, node) = seed_lineage(&f).await;

    f.service.enrich().await;

    assert_eq!(lineage_edges(&f, &node).await, 1);
}

#[tokio::test]
async fn scheduling_runs_a_pass_in_the_background() {
    let f = Fixture::new().await;
    let (subject, node) = seed_lineage(&f).await;

    f.service.schedule_enrichment(&HashSet::from([subject]));

    let started = std::time::Instant::now();
    while lineage_edges(&f, &node).await == 0 {
        assert!(started.elapsed() < Duration::from_secs(10), "no pass ran");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn nothing_changed_schedules_nothing() {
    let f = Fixture::new().await;
    let (_, node) = seed_lineage(&f).await;

    f.service.schedule_enrichment(&HashSet::new());
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_eq!(lineage_edges(&f, &node).await, 0);
}

impl Fixture {
    pub(super) async fn observing(config: PipedashConfig) -> Self {
        let f = Self::with_config(config).await;
        f.h.pipeline_service.set_observer(f.service.clone());
        f
    }
}

async fn until<F, Fut>(what: &str, mut done: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let started = std::time::Instant::now();
    while !done().await {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn no_registry() -> PipedashConfig {
    let mut config = PipedashConfig::default();
    config.correlation.registry_read = false;
    config
}

#[tokio::test]
async fn provenance_hook_links_the_build_run_even_without_registry_read() {
    let f = Fixture::observing(no_registry()).await;
    let image = digest('d');
    let commit = CommitSha::parse(&sha('b')).unwrap();
    let api = RepoId::parse("https://git.test/shop/api").unwrap();
    let config_repo = RepoId::parse("https://git.test/shop/config").unwrap();
    let (provider, _) = f
        .stub(|stub| {
            let build = format!("stub__{}__build", stub.provider_id);
            let mut run = stub_run(&build, 5, 60);
            run.correlation = Correlation::single_source(Some(api.clone()), Some(commit.clone()));
            stub.with_source_hosts(&["git.test"])
                .with_runs(vec![run])
                .with_provenance(Provenance {
                    pipeline_id: Some(build),
                    run_number: Some(5),
                    source: SourceObservation {
                        repo: api.clone(),
                        path_scopes: Vec::new(),
                        commit: Some(commit.clone()),
                    },
                })
        })
        .await;
    let mut build = pipeline(provider, "build", PipelineStatus::Success);
    build.correlation = Correlation::single_source(Some(api.clone()), None);
    let mut unit = pipeline(provider, "api-prod", PipelineStatus::Success);
    unit.correlation = Correlation {
        unit: UnitId::new("api-prod"),
        sources: vec![SourceObservation {
            repo: config_repo,
            path_scopes: vec![PathScope::new("apps/api")],
            commit: CommitSha::parse(&sha('c')),
        }],
        targets: vec![TargetObservation {
            cluster: ClusterId::from_uid("1").unwrap(),
            namespace: Some("shop".to_string()),
            name: "prod".to_string(),
            lifetime: TargetLifetime::Standing,
        }],
        consumed: vec![ConsumedArtifact {
            artifact: ArtifactObservation {
                repository: ImageRef::parse("registry.test/shop/api")
                    .unwrap()
                    .repository,
                digest: Digest::parse(&image),
                tag: None,
            },
            target: Some(0),
        }],
        ..Correlation::default()
    };
    f.h.repository
        .update_pipelines_cache(provider, &[build.clone(), unit.clone()])
        .await
        .unwrap();

    f.service
        .pipelines_applied_impl(provider, &[], &[build, unit])
        .await
        .unwrap();

    let build_id = format!("stub__{provider}__build");
    until("the build pipeline to join as an exact member", || async {
        f.service
            .store
            .conn()
            .list_application_members_by_pipeline(&build_id)
            .await
            .unwrap()
            .iter()
            .any(|member| {
                member.source == MemberSource::Exact.as_str()
                    && member.stage == Stage::Build.as_str()
            })
    })
    .await;
}

#[tokio::test]
async fn label_source_match_backfills_history_exactly_once() {
    let f = Fixture::with_config(no_registry()).await;
    let (provider, stub) = f.stub(|stub| stub).await;
    let build_id = format!("stub__{provider}__build");
    f.h.repository
        .update_pipelines_cache(
            provider,
            &[pipeline(provider, "build", PipelineStatus::Success)],
        )
        .await
        .unwrap();
    f.source(
        provider,
        &build_id,
        PIPELINE_LEVEL,
        "git.test/shop/api",
        None,
    )
    .await;
    let image = digest('d');
    f.consume(
        provider,
        &format!("stub__{provider}__api-prod"),
        &image,
        "registry.test/shop/api",
        Utc::now(),
    )
    .await;
    f.artifact(
        &image,
        "registry.test/shop/api",
        Utc::now(),
        Some("git.test/shop/api"),
    )
    .await;

    f.service.enrich().await;
    until("the history backfill", || async {
        stub.history_fetch_count() == 1
    })
    .await;
    f.service.enrich().await;
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert_eq!(stub.history_fetch_count(), 1);
}
