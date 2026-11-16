use std::sync::Arc;

use chrono::Utc;
use pipedash_plugin_api::{
    AppRef,
    ArtifactObservation,
    ClusterId,
    CommitSha,
    ConsumedArtifact,
    Digest,
    ImageRef,
    PathScope,
    RepoId,
    SourceObservation,
    TargetLifetime,
    TargetObservation,
    UnitId,
};

use super::state::OpenSuggestion;
use super::CorrelationService;
use crate::application::test_support::{
    pipeline,
    stub_run,
    Harness,
    StubProvider,
};
use crate::application::FetchResult;
use crate::domain::application::MemberSource;
use crate::domain::{
    Pipeline,
    PipelineRun,
    PipelineStatus,
};
use crate::infrastructure::database::{
    ApplicationMemberRow,
    ApplicationRow,
    CorrelationEdgeRow,
};

pub(super) struct Fx {
    pub h: Harness,
    pub svc: Arc<CorrelationService>,
}

pub(super) fn repo(path: &str) -> RepoId {
    let (host, rest) = path.split_once('/').unwrap();
    RepoId::from_host_path(host, rest).unwrap()
}

pub(super) fn sha(c: char) -> CommitSha {
    CommitSha::parse(&c.to_string().repeat(40)).unwrap()
}

pub(super) fn digest(c: char) -> Digest {
    Digest::parse(&format!("sha256:{}", c.to_string().repeat(64))).unwrap()
}

pub(super) fn source(repo_path: &str, scopes: &[&str], commit: Option<char>) -> SourceObservation {
    SourceObservation {
        repo: repo(repo_path),
        path_scopes: scopes.iter().map(|s| PathScope::new(s)).collect(),
        commit: commit.map(sha),
    }
}

pub(super) fn target(uid: &str, namespace: &str, name: &str) -> TargetObservation {
    TargetObservation {
        cluster: ClusterId::from_uid(uid).unwrap(),
        namespace: Some(namespace.to_string()),
        name: name.to_string(),
        lifetime: TargetLifetime::Standing,
    }
}

pub(super) fn artifact(
    repository: &str, digest: Option<Digest>, tag: Option<&str>,
) -> ArtifactObservation {
    ArtifactObservation {
        repository: ImageRef::parse(repository).unwrap().repository,
        digest,
        tag: tag.map(str::to_string),
    }
}

pub(super) fn consumed(
    repository: &str, digest: Digest, target: Option<usize>,
) -> ConsumedArtifact {
    ConsumedArtifact {
        artifact: artifact(repository, Some(digest), None),
        target,
    }
}

pub(super) fn app_ref(id: &str) -> AppRef {
    AppRef {
        authority: "catalog".into(),
        scope: "shop".into(),
        id: id.into(),
    }
}

pub(super) fn unit(provider: i64, name: &str) -> Pipeline {
    let mut p = pipeline(provider, name, PipelineStatus::Success);
    p.correlation.unit = UnitId::new(name);
    p
}

pub(super) fn builder(provider: i64, name: &str, sources: Vec<SourceObservation>) -> Pipeline {
    let mut p = pipeline(provider, name, PipelineStatus::Success);
    p.correlation.sources = sources;
    p
}

pub(super) fn run_with(
    pipeline_id: &str, number: i64, sources: Vec<SourceObservation>,
    produced: Vec<ArtifactObservation>,
) -> PipelineRun {
    let mut run = stub_run(pipeline_id, number, 60);
    run.correlation.sources = sources;
    run.correlation.produced = produced;
    run
}

pub(super) async fn settle(svc: &CorrelationService) {
    let drain = async {
        loop {
            let mut tasks = std::mem::take(
                &mut *svc
                    .background
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            );
            if tasks.is_empty() {
                return;
            }
            while tasks.join_next().await.is_some() {}
        }
    };
    tokio::time::timeout(std::time::Duration::from_secs(60), drain)
        .await
        .expect("correlation background work did not finish within 60s");
}

impl Fx {
    pub(super) async fn new() -> Self {
        let h = Harness::new().await;
        let svc = Arc::clone(&h.correlation_service);
        Self { h, svc }
    }

    pub(super) async fn provider(&self, name: &str) -> i64 {
        self.h.add_provider(name).await
    }

    pub(super) async fn apply(&self, provider: i64, pipelines: Vec<Pipeline>) {
        let old = self
            .h
            .repository
            .get_cached_pipelines(Some(provider))
            .await
            .unwrap();
        self.h
            .repository
            .update_pipelines_cache(provider, &pipelines)
            .await
            .unwrap();
        self.svc
            .pipelines_applied_at(provider, &old, &pipelines, Utc::now())
            .await
            .unwrap();
    }

    pub(super) async fn runs(&self, provider: i64, pipeline_id: &str, runs: Vec<PipelineRun>) {
        self.svc
            .runs_fetched_impl(provider, pipeline_id, &runs)
            .await
            .unwrap();
    }

    pub(super) async fn apps(&self) -> Vec<ApplicationRow> {
        let mut conn = self.svc.store.conn();
        let mut rows: Vec<ApplicationRow> = conn
            .list_applications()
            .await
            .unwrap()
            .into_iter()
            .filter(|a| a.archived_at.is_none())
            .collect();
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        rows
    }

    pub(super) async fn app(&self, name: &str) -> ApplicationRow {
        self.apps()
            .await
            .into_iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("no application named {name}"))
    }

    pub(super) async fn members(&self, application_id: i64) -> Vec<ApplicationMemberRow> {
        let mut conn = self.svc.store.conn();
        conn.list_application_members_by_application(application_id)
            .await
            .unwrap()
    }

    pub(super) async fn member_ids(&self, name: &str) -> Vec<String> {
        let app = self.app(name).await;
        self.members(app.id)
            .await
            .into_iter()
            .map(|m| m.pipeline_id)
            .collect()
    }

    pub(super) async fn edges(&self, rule: &str) -> Vec<CorrelationEdgeRow> {
        let mut conn = self.svc.store.conn();
        conn.list_edges_by_rule(rule).await.unwrap()
    }

    pub(super) async fn settle(&self) {
        settle(&self.svc).await;
    }

    pub(super) async fn open(&self) -> Vec<OpenSuggestion> {
        let mut conn = self.svc.store.conn();
        let world = self.svc.load_world(&mut conn).await.unwrap();
        self.svc.open_suggestions(&mut conn, &world).await.unwrap()
    }
}

#[tokio::test]
async fn build_links_to_deploy_by_digest() {
    let fx = Fx::new().await;
    let ci = fx.provider("ci").await;
    let gitops = fx.provider("gitops").await;
    let image = "registry.test/shop/api";

    let build = builder(ci, "build", vec![source("git.test/shop/api", &[], None)]);
    fx.apply(ci, vec![build.clone()]).await;
    let run = run_with(
        &build.id,
        1,
        vec![source("git.test/shop/api", &[], Some('1'))],
        vec![artifact(image, Some(digest('a')), None)],
    );
    fx.runs(ci, &build.id, vec![run]).await;

    let mut api = unit(gitops, "api-prod");
    api.correlation.sources = vec![source("git.test/shop/config", &["apps/api"], Some('2'))];
    api.correlation.targets = vec![target("1", "shop", "shop")];
    api.correlation.consumed = vec![consumed(image, digest('a'), Some(0))];
    let mut web = unit(gitops, "web-prod");
    web.correlation.sources = vec![source("git.test/shop/config", &["apps/web"], Some('2'))];
    web.correlation.targets = vec![target("1", "shop-web", "shop-web")];
    fx.apply(gitops, vec![api.clone(), web.clone()]).await;
    fx.svc.flush_events().await;

    let names: Vec<String> = fx.apps().await.into_iter().map(|a| a.name).collect();
    assert_eq!(names, ["api-prod", "web-prod"]);
    let app = fx.app("api-prod").await;
    let members = fx.members(app.id).await;
    let built = members.iter().find(|m| m.pipeline_id == build.id).unwrap();
    assert_eq!(built.stage, "build");
    assert_eq!(built.source, MemberSource::Exact.as_str());
    assert!(built.edge_id.is_some());
    let deployed = members.iter().find(|m| m.pipeline_id == api.id).unwrap();
    assert_eq!(deployed.stage, "deploy");
    assert_eq!(members.len(), 2);
    assert_eq!(fx.member_ids("web-prod").await, [web.id]);
    assert_eq!(fx.h.bus.count("applications-changed"), 1);
}

#[tokio::test]
async fn concurrent_refreshes_through_the_pipeline_service_are_observed() {
    let fx = Fx::new().await;
    fx.h.pipeline_service.set_observer(fx.svc.clone());
    let image = "registry.test/shop/api";
    let (ci, _) =
        fx.h.add_stub(|id| {
            StubProvider::new(id).returning(vec![builder(
                id,
                "build",
                vec![source("git.test/shop/api", &[], Some('1'))],
            )])
        })
        .await;
    let (gitops, _) =
        fx.h.add_stub(|id| {
            let mut api = unit(id, "api");
            api.correlation.sources = vec![source("git.test/shop/api", &["deploy"], Some('1'))];
            api.correlation.targets = vec![target("1", "shop", "shop")];
            api.correlation.consumed = vec![consumed(image, digest('a'), Some(0))];
            StubProvider::new(id).returning(vec![api])
        })
        .await;

    let (first, second) = tokio::join!(fx.h.fetch_directly(ci), fx.h.fetch_directly(gitops));
    assert!(matches!(first, FetchResult::Committed(Ok(_))));
    assert!(matches!(second, FetchResult::Committed(Ok(_))));
    fx.settle().await;

    let mut conn = fx.svc.store.conn();
    for provider in [ci, gitops] {
        let rows = conn
            .list_subject_identities_by_provider(provider)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
    assert_eq!(fx.apps().await.len(), 1);
    let before = conn
        .list_observation_windows_by_provider(gitops)
        .await
        .unwrap();

    fx.h.fetch_directly(gitops).await;
    fx.settle().await;
    let after = conn
        .list_observation_windows_by_provider(gitops)
        .await
        .unwrap();
    assert_eq!(after.len(), 1);
    assert!(after[0].ended_at > before[0].ended_at);
}
