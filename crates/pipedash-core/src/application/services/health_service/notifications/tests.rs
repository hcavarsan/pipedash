use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::TimeZone;
use pipedash_plugin_api::{
    ComponentFilter,
    ComponentHealth,
    FeedFormat,
    RateLimit,
    StatusSource,
};
use reqwest::Url;
use serde_json::{
    json,
    Value,
};

use super::super::feed_subject;
use super::scan::scan;
use super::*;
use crate::application::services::provider_service::ProviderFactory;
use crate::application::test_support::{
    pipeline,
    statuspage_body,
    stub_run,
    FeedStub,
    Gate,
    StubFactory,
    StubProvider,
};
use crate::domain::{
    DomainError,
    HealthLevel,
    PipelineRun,
    ProviderConfig,
};
use crate::infrastructure::HttpClientManager;

const BODY: &str = "acme/app · Provider 1";

fn at(minutes: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap() + chrono::Duration::minutes(minutes)
}

fn run(provider: i64, name: &str, status: PipelineStatus, minute: i64) -> Pipeline {
    let mut pipeline = pipeline(provider, name, status);
    pipeline.last_run = Some(at(minute));
    pipeline
}

fn summary(id: i64) -> ProviderSummary {
    serde_json::from_value(json!({
        "id": id, "name": format!("Provider {id}"), "provider_type": "github", "icon": null,
        "pipeline_count": 0, "last_updated": null, "refresh_interval": 30,
        "configured_repositories": [], "last_fetch_status": "success",
        "last_fetch_error": null, "last_fetch_at": null, "version": 1
    }))
    .unwrap()
}

fn updated(provider_id: i64, pipelines: Vec<Pipeline>) -> CoreEvent {
    CoreEvent::PipelinesUpdated {
        pipelines,
        provider_id,
        timestamp: 0,
    }
}

fn failed_event(pipelines: Vec<Pipeline>) -> CoreEvent {
    CoreEvent::PipelinesFailed { pipelines }
}

fn alert_event(key: &str, title: &str, recovered: bool) -> CoreEvent {
    CoreEvent::ProviderHealthAlert {
        alert_id: "alert".to_owned(),
        notification_key: key.to_owned(),
        provider_ids: vec![1],
        level: HealthLevel::MajorOutage,
        title: title.to_owned(),
        body: String::new(),
        link: None,
        recovered,
    }
}

fn titles(found: &[LocalNotification]) -> Vec<&str> {
    found.iter().map(|notice| notice.title.as_str()).collect()
}

fn reload(state: &NotificationState) -> NotificationState {
    serde_json::from_str(&serde_json::to_string(state).unwrap()).unwrap()
}

fn running_run(pipeline_id: &str, hours: i64) -> PipelineRun {
    serde_json::from_value(json!({
        "id": format!("{pipeline_id}__9"), "pipeline_id": pipeline_id, "run_number": 9,
        "status": "running", "started_at": Utc::now() - chrono::Duration::hours(hours),
        "logs_url": ""
    }))
    .unwrap()
}

fn summary_with(id: i64, signals: Vec<Value>, status_page: bool) -> ProviderSummary {
    let page = status_page.then(|| {
        json!({
            "name": "Example", "url": "https://status.example.com/",
            "checked_at": null, "error": null
        })
    });
    let mut value = serde_json::to_value(summary(id)).unwrap();
    value["health"] = json!({
        "level": "major_outage", "signals": signals, "next_attempt_at": null,
        "in_flight": false, "status_page": page
    });

    serde_json::from_value(value).unwrap()
}

fn status_signal() -> Value {
    json!({
        "kind": "status_page", "episode_id": "episode-1", "level": "major_outage",
        "evidence": "fresh", "observed_at": "2026-10-26T12:00:00Z",
        "title": "Incident inc1", "detail": null, "reported_by": "Example", "url": null,
        "components": [],
        "incidents": [{
            "id": "inc1", "name": "Incident inc1", "level": "major_outage",
            "phase": "investigating", "latest_update": null, "url": null,
            "started_at": null, "updated_at": "2026-10-26T12:00:00Z"
        }],
        "maintenances": [], "updated_at": "2026-10-26T12:00:00Z"
    })
}

fn instance_signal() -> Value {
    json!({
        "kind": "instance", "episode_id": "episode-2", "level": "major_outage",
        "evidence": "fresh", "observed_at": "2026-10-26T12:00:00Z",
        "title": "flux: scaled to zero", "detail": null, "reported_by": "Provider 1",
        "url": null,
        "components": [{"name": "flux", "level": "major_outage", "detail": "scaled to zero"}],
        "incidents": [], "maintenances": [], "updated_at": "2026-10-26T12:00:00Z"
    })
}

fn broken() -> ComponentHealth {
    ComponentHealth {
        name: "flux".to_owned(),
        level: HealthLevel::MajorOutage,
        detail: Some("scaled to zero".to_owned()),
    }
}

fn operational() -> Value {
    statuspage_body(&[("web", "operational")], json!([]))
}

fn outage() -> Value {
    statuspage_body(
        &[("web", "major_outage")],
        json!([{
            "id": "inc1", "name": "Incident inc1", "status": "investigating",
            "impact": "critical", "updated_at": "2026-10-22T10:00:00Z",
            "components": [{"id": "web"}],
            "incident_updates": [{"body": "We are investigating", "created_at": "2026-10-22T10:00:00Z"}]
        }]),
    )
}

fn source(feed: &FeedStub) -> StatusSource {
    StatusSource {
        name: "Example".to_owned(),
        page_url: Url::parse("https://status.example.com/").unwrap(),
        feed_url: feed.feed_url(),
        format: FeedFormat::Statuspage,
        components: ComponentFilter::All,
    }
}

struct Rig {
    factory: Arc<StubFactory>,
    http: HttpClientManager,
    state: NotificationState,
    controller: bool,
    status_pages: bool,
    version: i64,
}

impl Rig {
    fn new() -> Self {
        Self {
            factory: Arc::new(StubFactory::default()),
            http: HttpClientManager::new().unwrap(),
            state: NotificationState::default(),
            controller: false,
            status_pages: true,
            version: 1,
        }
    }

    fn stub(&self, id: i64, build: impl FnOnce(StubProvider) -> StubProvider) -> Arc<StubProvider> {
        self.factory.queue(build(StubProvider::new(id)))
    }

    fn config(&self, id: i64) -> ProviderConfig {
        ProviderConfig {
            id: Some(id),
            name: format!("Provider {id}"),
            provider_type: "github".to_owned(),
            token: "secret-token".to_owned(),
            config: HashMap::new(),
            refresh_interval: 30,
            version: Some(self.version),
        }
    }

    async fn scan(&mut self, ids: &[i64]) -> Vec<LocalNotification> {
        self.scan_within(ids, Duration::from_secs(20)).await
    }

    async fn scan_within(&mut self, ids: &[i64], deadline: Duration) -> Vec<LocalNotification> {
        let configs: Vec<ProviderConfig> = ids.iter().map(|id| self.config(*id)).collect();
        let factory = Arc::clone(&self.factory);
        let build: ProviderFactory = Arc::new(move |config| factory.provide(config));
        let controller = self.controller;

        scan(
            &configs,
            &build,
            &move |_| controller,
            &self.http,
            self.status_pages,
            &mut self.state,
            deadline,
        )
        .await
        .unwrap()
    }
}

fn fail(stub: &StubProvider, error: DomainError) {
    *stub.pipelines.lock().unwrap() = Err(error);
}

fn network() -> DomainError {
    DomainError::NetworkError("down".to_owned())
}

#[tokio::test]
async fn a_cold_scan_baselines_silently_and_later_failures_notify() {
    let mut rig = Rig::new();
    let stub = rig.stub(1, |stub| {
        stub.returning(vec![run(1, "api", PipelineStatus::Failed, 1)])
    });

    assert!(rig.scan(&[1]).await.is_empty());
    assert!(rig.scan(&[1]).await.is_empty());
    stub.set_pipelines(vec![run(1, "api", PipelineStatus::Failed, 2)]);
    let found = rig.scan(&[1]).await;

    assert_eq!(
        found,
        [LocalNotification {
            key: "pipeline:github__1__api".to_owned(),
            title: "api failed".to_owned(),
            body: BODY.to_owned(),
            pipeline_id: Some("github__1__api".to_owned()),
            provider_id: Some(1),
        }]
    );
}

#[tokio::test]
async fn rehydrated_state_keeps_the_baseline_and_holds_no_credentials() {
    let mut rig = Rig::new();
    let stub = rig.stub(1, |stub| {
        stub.returning(vec![run(1, "api", PipelineStatus::Success, 1)])
    });
    rig.scan(&[1]).await;

    rig.state = reload(&rig.state);
    stub.set_pipelines(vec![run(1, "api", PipelineStatus::Failed, 2)]);
    assert_eq!(titles(&rig.scan(&[1]).await), ["api failed"]);

    rig.state = reload(&rig.state);
    assert!(rig.scan(&[1]).await.is_empty());
    stub.set_pipelines(vec![run(1, "api", PipelineStatus::Failed, 3)]);
    assert_eq!(titles(&rig.scan(&[1]).await), ["api failed"]);
    assert!(!serde_json::to_string(&rig.state)
        .unwrap()
        .contains("secret-token"));
}

#[tokio::test]
async fn an_event_then_a_scan_notify_once() {
    let mut rig = Rig::new();
    let stub = rig.stub(1, |stub| {
        stub.returning(vec![run(1, "api", PipelineStatus::Success, 1)])
    });
    rig.scan(&[1]).await;
    let failed = run(1, "api", PipelineStatus::Failed, 2);

    let first = rig.state.observe_event(&failed_event(vec![failed.clone()]));
    let repeated = rig.state.observe_event(&failed_event(vec![failed.clone()]));
    stub.set_pipelines(vec![failed]);

    assert_eq!(titles(&first), ["api failed"]);
    assert!(repeated.is_empty());
    assert!(rig.scan(&[1]).await.is_empty());
}

#[tokio::test]
async fn a_scan_then_an_event_notify_once() {
    let mut rig = Rig::new();
    let stub = rig.stub(1, |stub| {
        stub.returning(vec![run(1, "api", PipelineStatus::Success, 1)])
    });
    rig.scan(&[1]).await;
    let failed = run(1, "api", PipelineStatus::Failed, 2);
    stub.set_pipelines(vec![failed.clone()]);

    let scanned = rig.scan(&[1]).await;

    assert_eq!(titles(&scanned), ["api failed"]);
    assert!(rig
        .state
        .observe_event(&failed_event(vec![failed.clone()]))
        .is_empty());
    assert!(rig
        .state
        .observe_event(&updated(1, vec![failed]))
        .is_empty());
}

#[test]
fn updated_and_failed_events_for_one_failure_notify_once() {
    let mut state = NotificationState::default();
    state.seed(&[run(1, "api", PipelineStatus::Success, 1)], &[summary(1)]);
    let failed = run(1, "api", PipelineStatus::Failed, 2);

    let first = state.observe_event(&updated(1, vec![failed.clone()]));
    let second = state.observe_event(&failed_event(vec![failed]));

    assert_eq!(titles(&first), ["api failed"]);
    assert_eq!(first[0].body, BODY);
    assert!(second.is_empty());
}

#[test]
fn seeding_is_silent_and_replaces_older_state() {
    let failed = run(1, "api", PipelineStatus::Failed, 1);
    let mut state = NotificationState::default();
    state.seed(&[run(2, "web", PipelineStatus::Success, 1)], &[summary(2)]);
    state.seed(std::slice::from_ref(&failed), &[summary(1)]);

    assert!(state
        .observe_event(&failed_event(vec![failed.clone()]))
        .is_empty());
    assert!(state.observe_event(&updated(1, vec![failed])).is_empty());
    assert!(!state.pipelines.contains_key(&2));
    let next = run(1, "api", PipelineStatus::Failed, 2);
    assert_eq!(
        titles(&state.observe_event(&updated(1, vec![next]))),
        ["api failed"]
    );
}

#[test]
fn an_overdue_event_notifies_once_per_run() {
    let mut state = NotificationState::default();
    state.seed(&[run(1, "api", PipelineStatus::Running, 1)], &[summary(1)]);
    let mut slow = run(1, "api", PipelineStatus::Running, 1);
    slow.overdue_run = Some("run-1".to_owned());

    let first = state.observe_event(&updated(1, vec![slow.clone()]));
    let repeated = state.observe_event(&updated(1, vec![slow.clone()]));
    slow.overdue_run = Some("run-2".to_owned());
    let next = state.observe_event(&updated(1, vec![slow]));

    assert_eq!(titles(&first), ["api is running longer than usual"]);
    assert!(repeated.is_empty());
    assert_eq!(next.len(), 1);
}

#[tokio::test]
async fn a_scan_flags_a_running_pipeline_past_its_usual_duration_once() {
    let mut rig = Rig::new();
    let id = "github__1__api";
    let mut runs = vec![running_run(id, 3)];
    runs.extend((1..=3).map(|number| stub_run(id, number, 600)));
    let stub = rig.stub(1, |stub| stub.with_runs(runs));
    stub.set_pipelines(vec![run(1, "api", PipelineStatus::Success, 1)]);
    assert!(rig.scan(&[1]).await.is_empty());

    stub.set_pipelines(vec![run(1, "api", PipelineStatus::Running, 2)]);
    let found = rig.scan(&[1]).await;

    assert_eq!(titles(&found), ["api is running longer than usual"]);
    assert_eq!(found[0].key, "pipeline:github__1__api");
    assert!(rig.scan(&[1]).await.is_empty());
}

#[tokio::test]
async fn failed_scans_keep_the_baseline_and_an_outage_recovers_once() {
    let mut rig = Rig::new();
    let stub = rig.stub(1, |stub| {
        stub.returning(vec![run(1, "api", PipelineStatus::Failed, 1)])
    });
    assert!(rig.scan(&[1]).await.is_empty());

    fail(&stub, network());
    assert!(rig.scan(&[1]).await.is_empty());
    let outage = rig.scan(&[1]).await;
    assert_eq!(titles(&outage), ["Can't reach Provider 1"]);
    assert_eq!(outage[0].key, "connection:1");
    assert_eq!(outage[0].provider_id, Some(1));
    assert!(rig.scan(&[1]).await.is_empty());

    stub.set_pipelines(vec![run(1, "api", PipelineStatus::Failed, 1)]);
    let recovered = rig.scan(&[1]).await;
    assert_eq!(titles(&recovered), ["Provider 1 is reachable again"]);
    assert!(rig.scan(&[1]).await.is_empty());

    stub.set_pipelines(vec![run(1, "api", PipelineStatus::Failed, 2)]);
    assert_eq!(titles(&rig.scan(&[1]).await), ["api failed"]);
}

#[tokio::test]
async fn an_outage_survives_a_reload_and_is_not_repeated() {
    let mut rig = Rig::new();
    let stub = rig.stub(1, |stub| stub);
    rig.scan(&[1]).await;
    fail(&stub, network());
    rig.scan(&[1]).await;
    assert_eq!(rig.scan(&[1]).await.len(), 1);

    rig.state = reload(&rig.state);

    assert!(rig.scan(&[1]).await.is_empty());
    stub.set_pipelines(vec![run(1, "api", PipelineStatus::Success, 1)]);
    assert_eq!(
        titles(&rig.scan(&[1]).await),
        ["Provider 1 is reachable again"]
    );
}

#[tokio::test]
async fn a_failed_scan_of_another_kind_never_fabricates_a_recovery() {
    let mut rig = Rig::new();
    let stub = rig.stub(1, |stub| stub);
    rig.scan(&[1]).await;
    fail(&stub, network());
    rig.scan(&[1]).await;
    assert_eq!(rig.scan(&[1]).await.len(), 1);

    fail(
        &stub,
        DomainError::AuthenticationFailed("revoked".to_owned()),
    );
    assert!(rig.scan(&[1]).await.is_empty());
    stub.set_pipelines(vec![run(1, "api", PipelineStatus::Success, 1)]);

    assert!(rig.scan(&[1]).await.is_empty());
    assert!(rig.scan(&[1]).await.is_empty());
}

#[tokio::test]
async fn a_changed_provider_version_starts_a_new_connection_episode() {
    let mut rig = Rig::new();
    let stub = rig.stub(1, |stub| stub);
    rig.scan(&[1]).await;
    fail(&stub, network());
    rig.scan(&[1]).await;
    assert_eq!(rig.scan(&[1]).await.len(), 1);

    rig.version = 2;
    stub.set_pipelines(vec![run(1, "api", PipelineStatus::Success, 1)]);

    assert!(rig.scan(&[1]).await.is_empty());
    assert!(rig.scan(&[1]).await.is_empty());
}

#[tokio::test]
async fn removed_providers_lose_their_state_and_return_as_a_fresh_baseline() {
    let mut rig = Rig::new();
    rig.stub(1, |stub| stub);
    let second = rig.stub(2, |stub| stub);
    rig.scan(&[1, 2]).await;
    assert!(rig.state.providers.contains_key(&2));

    rig.scan(&[1]).await;
    assert!(!rig.state.providers.contains_key(&2));
    assert!(!rig.state.pipelines.contains_key(&2));
    assert!(rig.state.pipelines.contains_key(&1));

    second.set_pipelines(vec![run(2, "web", PipelineStatus::Failed, 5)]);
    assert!(rig.scan(&[1, 2]).await.is_empty());
}

#[test]
fn a_removed_provider_event_and_retain_drop_state() {
    let mut state = NotificationState::default();
    state.seed(
        &[
            run(1, "api", PipelineStatus::Success, 1),
            run(2, "web", PipelineStatus::Success, 1),
            run(3, "app", PipelineStatus::Success, 1),
        ],
        &[summary(1), summary(2), summary(3)],
    );

    state.observe_event(&CoreEvent::ProviderRemoved {
        provider: summary(2),
        timestamp: 0,
    });
    state.retain_providers(&HashSet::from([1]));

    assert_eq!(state.pipelines.keys().copied().collect::<Vec<_>>(), [1]);
    assert_eq!(state.providers.keys().copied().collect::<Vec<_>>(), [1]);
}

#[tokio::test]
async fn a_status_page_outage_alerts_once_and_recovers_only_after_calm_checks() {
    let feed = FeedStub::start().await;
    let mut rig = Rig::new();
    rig.stub(1, |stub| stub.with_source(source(&feed)));
    let key = feed_subject(&(feed.feed_url(), FeedFormat::Statuspage));

    feed.respond_json(operational());
    assert!(rig.scan(&[1]).await.is_empty());
    feed.respond_json(outage());
    let found = rig.scan(&[1]).await;
    assert_eq!(titles(&found), ["Incident inc1"]);
    assert_eq!(found[0].key, key);
    assert_eq!(found[0].body, "We are investigating");
    assert_eq!(found[0].provider_id, Some(1));
    assert!(rig.scan(&[1]).await.is_empty());
    assert!(rig
        .state
        .observe_event(&alert_event(&key, "Incident inc1", false))
        .is_empty());

    feed.respond_json(operational());
    assert!(rig.scan(&[1]).await.is_empty());
    feed.respond(500, "");
    assert!(rig.scan(&[1]).await.is_empty());
    feed.respond_json(operational());
    assert!(rig.scan(&[1]).await.is_empty());
    let recovered = rig.scan(&[1]).await;
    assert_eq!(titles(&recovered), ["Example is back to normal"]);
    assert!(rig.scan(&[1]).await.is_empty());
}

#[tokio::test]
async fn a_status_page_outage_seen_first_by_an_event_is_not_repeated_by_the_scan() {
    let feed = FeedStub::start().await;
    let mut rig = Rig::new();
    rig.stub(1, |stub| stub.with_source(source(&feed)));
    let key = feed_subject(&(feed.feed_url(), FeedFormat::Statuspage));
    feed.respond_json(operational());
    rig.scan(&[1]).await;

    let event = rig
        .state
        .observe_event(&alert_event(&key, "Incident inc1", false));
    assert_eq!(titles(&event), ["Incident inc1"]);
    feed.respond_json(outage());
    assert!(rig.scan(&[1]).await.is_empty());

    rig.state = reload(&rig.state);
    feed.respond_json(operational());
    assert!(rig.scan(&[1]).await.is_empty());
    assert_eq!(titles(&rig.scan(&[1]).await), ["Example is back to normal"]);
    assert!(rig
        .state
        .observe_event(&alert_event(&key, "Example is back to normal", true))
        .is_empty());
}

#[tokio::test]
async fn a_status_page_outage_present_at_the_first_scan_is_a_silent_baseline() {
    let feed = FeedStub::start().await;
    let mut rig = Rig::new();
    rig.stub(1, |stub| stub.with_source(source(&feed)));
    feed.respond_json(outage());

    assert!(rig.scan(&[1]).await.is_empty());
    assert!(rig.scan(&[1]).await.is_empty());
    feed.respond_json(operational());
    assert!(rig.scan(&[1]).await.is_empty());
    assert!(rig.scan(&[1]).await.is_empty());
    assert!(rig.scan(&[1]).await.is_empty());
}

#[tokio::test]
async fn controller_health_alerts_and_recovers_without_trusting_failed_checks() {
    let mut rig = Rig::new();
    rig.controller = true;
    let stub = rig.stub(1, |stub| stub);
    let broken = ComponentHealth {
        name: "flux".to_owned(),
        level: HealthLevel::MajorOutage,
        detail: Some("scaled to zero".to_owned()),
    };
    stub.set_health(Ok(Vec::new()));
    assert!(rig.scan(&[1]).await.is_empty());

    stub.set_health(Ok(vec![broken]));
    let found = rig.scan(&[1]).await;
    assert_eq!(titles(&found), ["flux: scaled to zero"]);
    assert_eq!(found[0].key, "instance:1");
    assert_eq!(
        found[0].body,
        "Reported by the controller health check of Provider 1."
    );

    stub.set_health(Ok(Vec::new()));
    assert!(rig.scan(&[1]).await.is_empty());
    stub.set_health(Err(network()));
    assert!(rig.scan(&[1]).await.is_empty());
    stub.set_health(Ok(Vec::new()));
    assert!(rig.scan(&[1]).await.is_empty());
    assert_eq!(
        titles(&rig.scan(&[1]).await),
        ["Provider 1 controllers recovered"]
    );
}

#[tokio::test]
async fn a_provider_close_to_its_quota_is_skipped_until_the_reset() {
    let mut rig = Rig::new();
    let stub = rig.stub(1, |stub| stub);
    *stub.rate_limit.lock().unwrap() = Some(RateLimit {
        limit: 5000,
        remaining: 10,
        reset_at: Utc::now() + chrono::Duration::hours(1),
    });

    rig.scan(&[1]).await;
    *stub.rate_limit.lock().unwrap() = None;
    rig.scan(&[1]).await;

    assert_eq!(stub.fetch_count(), 0);
}

#[tokio::test]
async fn a_scan_over_budget_returns_finished_notifications() {
    let gate = Gate::new();
    gate.open();
    let mut rig = Rig::new();
    let first = rig.stub(1, |stub| {
        stub.returning(vec![run(1, "api", PipelineStatus::Success, 1)])
    });
    rig.stub(2, |stub| stub.gated(&gate));
    rig.scan(&[1, 2]).await;

    gate.close();
    first.set_pipelines(vec![run(1, "api", PipelineStatus::Failed, 2)]);
    let found = rig.scan_within(&[1, 2], Duration::from_millis(300)).await;

    assert_eq!(titles(&found), ["api failed"]);
}

#[tokio::test]
async fn providers_starved_by_hanging_ones_go_first_on_the_next_scan() {
    let gate = Gate::new();
    let mut rig = Rig::new();
    for id in 1..=4 {
        rig.stub(id, |stub| stub.gated(&gate));
    }
    let last = rig.stub(5, |stub| stub);
    let ids = [1, 2, 3, 4, 5];

    rig.scan_within(&ids, Duration::from_millis(300)).await;
    assert_eq!(last.fetch_count(), 0);
    rig.scan_within(&ids, Duration::from_millis(300)).await;

    assert_eq!(last.fetch_count(), 1);
}

#[tokio::test]
async fn a_bad_proxy_is_a_fatal_configuration_error() {
    let mut state = NotificationState::default();

    let result = crate::application::refresh_notifications(
        &[],
        "not a proxy url",
        true,
        &mut state,
        Duration::from_secs(1),
    )
    .await;

    assert!(result.is_err());
}

fn overdue_runs(pipeline_id: &str) -> Vec<PipelineRun> {
    let mut runs = vec![running_run(pipeline_id, 3)];
    runs.extend((1..=3).map(|number| stub_run(pipeline_id, number, 600)));
    runs
}

#[tokio::test]
async fn running_pipelines_beyond_the_per_scan_cap_are_covered_by_later_scans() {
    let mut rig = Rig::new();
    let many: Vec<Pipeline> = (0..30)
        .map(|n| run(1, &format!("p{n:02}"), PipelineStatus::Running, 1))
        .collect();
    rig.stub(1, |stub| {
        stub.returning(many)
            .with_runs(overdue_runs("github__1__p00"))
    });
    let checked = |rig: &Rig| {
        rig.state.pipelines[&1]
            .values()
            .filter(|mark| mark.checked_at.is_some())
            .count()
    };

    assert!(rig.scan(&[1]).await.is_empty());
    assert_eq!(checked(&rig), 25);
    assert!(rig.scan(&[1]).await.is_empty());
    assert_eq!(checked(&rig), 30);
    assert!(rig.scan(&[1]).await.is_empty());
}

#[tokio::test]
async fn a_pipeline_already_overdue_at_the_first_scan_stays_silent() {
    let mut rig = Rig::new();
    rig.stub(1, |stub| {
        stub.returning(vec![run(1, "api", PipelineStatus::Running, 1)])
            .with_runs(overdue_runs("github__1__api"))
    });

    assert!(rig.scan(&[1]).await.is_empty());
    assert!(rig.scan(&[1]).await.is_empty());
}

#[tokio::test]
async fn hanging_run_history_does_not_block_outage_checks_within_the_deadline() {
    let gate = Gate::new();
    gate.open();
    let mut rig = Rig::new();
    rig.controller = true;
    let stub = rig.stub(1, |stub| {
        stub.run_gated(&gate)
            .returning(vec![run(1, "api", PipelineStatus::Running, 1)])
    });
    stub.set_health(Ok(Vec::new()));
    rig.scan(&[1]).await;

    gate.close();
    stub.set_health(Ok(vec![broken()]));
    let found = rig.scan_within(&[1], Duration::from_millis(300)).await;

    assert_eq!(titles(&found), ["flux: scaled to zero"]);
}

#[tokio::test]
async fn disabling_status_pages_drops_event_created_feed_state() {
    let feed = FeedStub::start().await;
    let mut rig = Rig::new();
    rig.stub(1, |stub| stub.with_source(source(&feed)));
    let key = feed_subject(&(feed.feed_url(), FeedFormat::Statuspage));
    feed.respond_json(operational());
    rig.scan(&[1]).await;
    rig.state
        .observe_event(&alert_event(&key, "Incident inc1", false));
    assert!(rig.state.alerts.has(&key));

    rig.status_pages = false;
    let requests = feed.request_count();
    assert!(rig.scan(&[1]).await.is_empty());
    assert_eq!(feed.request_count(), requests);
    assert!(!rig.state.alerts.has(&key));
    assert!(!rig.state.delivered.contains_key(&key));

    rig.status_pages = true;
    for _ in 0..3 {
        assert!(rig.scan(&[1]).await.is_empty());
    }
}

#[tokio::test]
async fn a_status_page_outage_after_a_healthy_opt_in_alerts_on_the_first_scan() {
    let feed = FeedStub::start().await;
    let mut rig = Rig::new();
    rig.stub(1, |stub| stub.with_source(source(&feed)));
    rig.state.seed(&[], &[summary_with(1, Vec::new(), true)]);
    rig.state = reload(&rig.state);
    feed.respond_json(outage());

    assert_eq!(titles(&rig.scan(&[1]).await), ["Incident inc1"]);
    assert!(rig.scan(&[1]).await.is_empty());
}

#[tokio::test]
async fn a_status_page_outage_present_at_opt_in_stays_silent() {
    let feed = FeedStub::start().await;
    let mut rig = Rig::new();
    rig.stub(1, |stub| stub.with_source(source(&feed)));
    rig.state
        .seed(&[], &[summary_with(1, vec![status_signal()], true)]);
    rig.state = reload(&rig.state);
    feed.respond_json(outage());

    assert!(rig.scan(&[1]).await.is_empty());
    assert!(rig.scan(&[1]).await.is_empty());
    feed.respond_json(operational());
    for _ in 0..3 {
        assert!(rig.scan(&[1]).await.is_empty());
    }
}

#[tokio::test]
async fn a_controller_outage_after_a_healthy_opt_in_alerts_on_the_first_scan() {
    let mut rig = Rig::new();
    rig.controller = true;
    let stub = rig.stub(1, |stub| stub);
    stub.set_health(Ok(vec![broken()]));
    rig.state.seed(&[], &[summary_with(1, Vec::new(), false)]);
    rig.state = reload(&rig.state);

    assert_eq!(titles(&rig.scan(&[1]).await), ["flux: scaled to zero"]);
    assert!(rig.scan(&[1]).await.is_empty());
}

#[tokio::test]
async fn a_controller_outage_present_at_opt_in_stays_silent() {
    let mut rig = Rig::new();
    rig.controller = true;
    let stub = rig.stub(1, |stub| stub);
    stub.set_health(Ok(vec![broken()]));
    rig.state
        .seed(&[], &[summary_with(1, vec![instance_signal()], false)]);
    rig.state = reload(&rig.state);

    assert!(rig.scan(&[1]).await.is_empty());
    assert!(rig.scan(&[1]).await.is_empty());
    stub.set_health(Ok(Vec::new()));
    for _ in 0..3 {
        assert!(rig.scan(&[1]).await.is_empty());
    }
}
