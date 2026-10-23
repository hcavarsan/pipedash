use std::future::Future;
use std::task::Poll;

use pipedash_plugin_api::ComponentFilter;
use serde_json::{
    json,
    Value,
};

use super::*;
use crate::application::test_support::{
    statuspage_body,
    FeedRequest,
    FeedStub,
    Gate,
    Harness,
    StubProvider,
};
use crate::application::RefreshMode;
use crate::event::{
    AlertLink,
    AlertLinkKind,
    CoreEvent,
};
use crate::infrastructure::GeneralSettingsUpdate;

fn state(generation: Generation, outage_floor: bool) -> ProviderHealthState {
    ProviderHealthState {
        generation,
        level: if outage_floor {
            HealthLevel::MajorOutage
        } else {
            HealthLevel::Operational
        },
        signals: Vec::new(),
        status_page: None,
        outage_floor,
    }
}

#[test]
fn floors_only_cover_providers_in_a_fresh_major_outage() {
    let (handle, receivers) = HealthHandle::channel();
    let generation = Generation::next();
    receivers.snapshot.send_replace(Arc::new(HealthSnapshot {
        providers: HashMap::from([(1, state(generation, true)), (2, state(generation, false))]),
    }));

    let floors = handle.floors(Duration::from_secs(120));

    assert_eq!(floors, HashMap::from([(1, Duration::from_secs(120))]));
    assert_eq!(handle.snapshot().providers.len(), 2);
    assert!(HealthHandle::channel()
        .0
        .floors(Duration::from_secs(1))
        .is_empty());
}

#[test]
fn sending_is_non_blocking_and_ignores_a_closed_channel() {
    let (handle, receivers) = HealthHandle::channel();
    handle.send(HealthInput::SettingsChanged);
    let mut inputs = receivers.inputs;

    assert!(matches!(
        inputs.try_recv(),
        Ok(HealthInput::SettingsChanged)
    ));
    drop(inputs);
    handle.send(HealthInput::ForceFeeds {
        min_age: Duration::from_secs(15),
    });
}

#[test]
fn a_feed_failure_backs_off_from_a_minute_up_to_half_an_hour() {
    let start = Instant::now();
    let mut feed = FeedState::new(start);
    let mut waits = Vec::new();
    for _ in 0..7 {
        feed.failed(start, FEED_UNREACHABLE, None);
        waits.push(feed.next_try.duration_since(start).as_secs());
    }
    feed.failed(start, FEED_UNREACHABLE, Some(Duration::from_secs(3600)));

    assert_eq!(waits, [60, 120, 240, 480, 960, 1800, 1800]);
    assert_eq!(feed.next_try.duration_since(start).as_secs(), 3600);
    assert_eq!(feed.failures, 8);
}

fn secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

async fn advance(seconds: u64) {
    tokio::time::advance(secs(seconds) + Duration::from_millis(1)).await;
}

async fn quiesce() {
    for _ in 0..25 {
        tokio::task::yield_now().await;
        std::thread::sleep(Duration::from_millis(2));
    }
}

async fn until(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !ready() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::task::yield_now().await;
    }
}

async fn busy<T>(work: impl Future<Output = T>) -> T {
    let mut work = std::pin::pin!(work);
    loop {
        if let Poll::Ready(value) = futures::poll!(work.as_mut()) {
            return value;
        }
        tokio::task::yield_now().await;
    }
}

fn operational() -> Value {
    statuspage_body(&[("web", "operational"), ("api", "operational")], json!([]))
}

fn outage(status: &str) -> Value {
    statuspage_body(&[("web", status), ("api", "operational")], json!([]))
}

fn incident(id: &str, impact: &str) -> Value {
    json!({
        "id": id, "name": format!("Incident {id}"), "status": "investigating",
        "impact": impact, "updated_at": "2026-10-22T10:00:00Z",
        "components": [{"id": "web"}],
        "incident_updates": [{"body": "We are investigating", "created_at": "2026-10-22T10:00:00Z"}]
    })
}

fn settings(status_pages_enabled: bool) -> GeneralSettingsUpdate {
    GeneralSettingsUpdate {
        metrics_enabled: true,
        default_refresh_interval: 30,
        status_pages_enabled,
    }
}

struct Prov {
    id: i64,
    generation: Generation,
    stub: Arc<StubProvider>,
}

#[derive(Debug, Clone)]
struct Seen {
    alert_id: String,
    key: String,
    provider_ids: Vec<i64>,
    level: HealthLevel,
    title: String,
    link: Option<AlertLink>,
    recovered: bool,
}

struct World {
    h: Harness,
    feed: FeedStub,
}

impl World {
    async fn new() -> Self {
        Self {
            h: Harness::with_health().await,
            feed: FeedStub::start().await,
        }
    }

    fn source(&self, filter: ComponentFilter) -> StatusSource {
        StatusSource {
            name: "Example".to_owned(),
            page_url: Url::parse("https://status.example.com/").unwrap(),
            feed_url: self.feed.feed_url(),
            format: FeedFormat::Statuspage,
            components: filter,
        }
    }

    async fn provider_with(
        &self, source: Option<StatusSource>, build: impl FnOnce(StubProvider) -> StubProvider,
    ) -> Prov {
        let (id, stub) = self
            .h
            .add_stub_every(0, |id| {
                let stub = StubProvider::new(id);
                build(match source {
                    Some(source) => stub.with_source(source),
                    None => stub,
                })
            })
            .await;
        let generation = self
            .h
            .provider_service
            .acquire(id)
            .await
            .unwrap()
            .generation;

        Prov {
            id,
            generation,
            stub,
        }
    }

    async fn provider(&self, filter: ComponentFilter) -> Prov {
        self.provider_with(Some(self.source(filter)), |stub| stub)
            .await
    }

    async fn plain(&self) -> Prov {
        self.provider_with(None, |stub| stub).await
    }

    async fn instance_provider(&self, build: impl FnOnce(StubProvider) -> StubProvider) -> Prov {
        let prov = self.provider_with(None, build).await;
        let config = self
            .h
            .provider_service
            .get_provider_config(prov.id)
            .await
            .unwrap();
        self.h.health.send(HealthInput::ProvidersChanged {
            changed: vec![ProviderHealthInput {
                id: prov.id,
                generation: Some(prov.generation),
                display_name: config.display_name().to_string(),
                source: None,
                has_instance_checks: true,
            }],
        });

        prov
    }

    async fn start(&self) {
        tokio::time::pause();
        self.h.health_service.as_ref().unwrap().start().await;
    }

    fn service(&self) -> &Arc<ProviderHealthService> {
        self.h.health_service.as_ref().unwrap()
    }

    fn outcome(&self, prov: &Prov, kind: Option<FailureKind>, failures: u32) {
        self.h.health.send(HealthInput::FetchOutcome {
            provider_id: prov.id,
            generation: prov.generation,
            view: ScheduleView {
                failures,
                last_failure: kind,
                next_attempt_in: Duration::ZERO,
                in_flight: false,
            },
            kind,
        });
    }

    fn state(&self, id: i64) -> Option<ProviderHealthState> {
        self.h.health.snapshot().providers.get(&id).cloned()
    }

    fn signals(&self, id: i64) -> Vec<HealthSignal> {
        self.state(id).map(|s| s.signals).unwrap_or_default()
    }

    fn level(&self, id: i64) -> HealthLevel {
        self.state(id)
            .map_or(HealthLevel::Operational, |state| state.level)
    }

    fn alerts(&self) -> Vec<Seen> {
        self.h
            .bus
            .events()
            .into_iter()
            .filter_map(|event| match event {
                CoreEvent::ProviderHealthAlert {
                    alert_id,
                    notification_key,
                    provider_ids,
                    level,
                    title,
                    link,
                    recovered,
                    ..
                } => Some(Seen {
                    alert_id,
                    key: notification_key,
                    provider_ids,
                    level,
                    title,
                    link,
                    recovered,
                }),
                _ => None,
            })
            .collect()
    }

    fn requests(&self) -> Vec<FeedRequest> {
        self.feed.requests()
    }

    async fn first_poll(&self) {
        until("the first feed request", || self.feed.request_count() >= 1).await;
        quiesce().await;
    }

    async fn next_poll(&self) {
        self.poll_after(120).await;
    }

    async fn poll_after(&self, seconds: u64) {
        let expected = self.feed.request_count() + 1;
        advance(seconds).await;
        until("the next feed request", || {
            self.feed.request_count() == expected
        })
        .await;
        quiesce().await;
    }

    fn stale(&self, id: i64) -> bool {
        self.signals(id)
            .first()
            .is_some_and(|signal| signal.evidence == EvidenceView::Stale)
    }
}

#[tokio::test]
async fn feeds_are_polled_every_two_minutes_and_every_minute_while_a_provider_fails() {
    let w = World::new().await;
    let p = w.provider(ComponentFilter::All).await;
    w.start().await;
    w.first_poll().await;

    advance(119).await;
    quiesce().await;
    assert_eq!(w.feed.request_count(), 1);
    advance(1).await;
    until("the poll at 120 s", || w.feed.request_count() == 2).await;
    quiesce().await;

    w.outcome(&p, Some(FailureKind::Network), 1);
    quiesce().await;
    advance(59).await;
    quiesce().await;
    assert_eq!(w.feed.request_count(), 2);
    w.poll_after(1).await;
    assert_eq!(w.feed.request_count(), 3);
    w.poll_after(60).await;
    assert_eq!(w.feed.request_count(), 4);

    w.outcome(&p, None, 0);
    quiesce().await;
    advance(119).await;
    quiesce().await;
    assert_eq!(w.feed.request_count(), 4);
    w.poll_after(1).await;
    assert_eq!(w.feed.request_count(), 5);
}

#[tokio::test]
async fn feeds_are_not_polled_while_idle_and_refresh_when_active_again() {
    let w = World::new().await;
    w.provider(ComponentFilter::All).await;
    w.start().await;
    w.first_poll().await;

    w.h.refresh_manager.set_mode(RefreshMode::Idle);
    quiesce().await;
    advance(120).await;
    quiesce().await;
    advance(600).await;
    quiesce().await;
    assert_eq!(w.feed.request_count(), 1);

    w.h.refresh_manager.set_mode(RefreshMode::Active);
    until("a refresh on activation", || w.feed.request_count() == 2).await;
    quiesce().await;
    w.h.refresh_manager.set_mode(RefreshMode::Idle);
    quiesce().await;
    w.h.refresh_manager.set_mode(RefreshMode::Active);
    quiesce().await;
    assert_eq!(w.feed.request_count(), 2);
}

#[tokio::test]
async fn forcing_the_feeds_refreshes_only_those_older_than_the_minimum_age() {
    let w = World::new().await;
    w.provider(ComponentFilter::All).await;
    w.start().await;
    w.first_poll().await;

    advance(10).await;
    w.h.health
        .send(HealthInput::ForceFeeds { min_age: secs(15) });
    quiesce().await;
    assert_eq!(w.feed.request_count(), 1);

    advance(10).await;
    w.h.health
        .send(HealthInput::ForceFeeds { min_age: secs(15) });
    until("the forced refresh", || w.feed.request_count() == 2).await;
    quiesce().await;
    w.h.health
        .send(HealthInput::ForceFeeds { min_age: secs(15) });
    quiesce().await;
    assert_eq!(w.feed.request_count(), 2);
}

#[tokio::test]
async fn a_304_confirms_the_previous_parse_and_keeps_its_signals_fresh() {
    let w = World::new().await;
    w.feed.respond_json(outage("major_outage"));
    w.feed.set_etag(Some("\"v1\""));
    let p = w.provider(ComponentFilter::All).await;
    w.start().await;
    w.first_poll().await;
    until("the outage", || !w.signals(p.id).is_empty()).await;
    let first_check = w.state(p.id).unwrap().status_page.unwrap().checked_at;

    for _ in 0..11 {
        w.next_poll().await;
    }

    let requests = w.requests();
    assert_eq!(requests[0].path, "/api/v2/summary.json");
    assert_eq!(requests[0].if_none_match, None);
    assert_eq!(requests[0].accept.as_deref(), Some("application/json"));
    assert!(requests[1..]
        .iter()
        .all(|request| request.if_none_match.as_deref() == Some("\"v1\"")));
    let state = w.state(p.id).unwrap();
    assert_eq!(state.signals.len(), 1);
    assert_eq!(state.signals[0].evidence, EvidenceView::Fresh);
    assert_eq!(state.level, HealthLevel::MajorOutage);
    assert!(state.status_page.unwrap().checked_at > first_check);
}

#[tokio::test]
async fn a_failing_feed_backs_off_and_never_changes_what_was_last_parsed() {
    let w = World::new().await;
    w.feed.respond(500, "oops");
    let p = w.provider(ComponentFilter::All).await;
    w.start().await;
    w.first_poll().await;

    let waits = [60, 120, 240, 480, 960, 1800, 1800];
    for (index, wait) in waits.iter().enumerate() {
        advance(wait - 1).await;
        quiesce().await;
        assert_eq!(w.feed.request_count(), index + 1);
        w.poll_after(1).await;
    }

    let requests = w.requests();
    let gaps: Vec<u64> = requests
        .windows(2)
        .map(|pair| pair[1].at.duration_since(pair[0].at).as_secs())
        .collect();
    assert_eq!(gaps, waits);
    let state = w.state(p.id).unwrap();
    let page = state.status_page.unwrap();
    assert_eq!(page.error.as_deref(), Some(FEED_UNREACHABLE));
    assert_eq!(page.checked_at, None);
    assert!(state.signals.is_empty());
}

#[tokio::test]
async fn a_feed_asking_for_a_longer_wait_is_obeyed() {
    let w = World::new().await;
    w.feed.respond(503, "busy");
    w.feed.with_header("Retry-After", "900");
    w.provider(ComponentFilter::All).await;
    w.start().await;
    w.first_poll().await;

    advance(899).await;
    quiesce().await;
    assert_eq!(w.feed.request_count(), 1);
    w.poll_after(1).await;
    assert_eq!(w.feed.request_count(), 2);
}

#[tokio::test]
async fn a_body_that_is_not_a_status_feed_is_an_invalid_response() {
    let w = World::new().await;
    w.feed.respond(200, "<html>maintenance</html>");
    let p = w.provider(ComponentFilter::All).await;
    w.start().await;
    w.first_poll().await;

    let page = w.state(p.id).unwrap().status_page.unwrap();

    assert_eq!(page.error.as_deref(), Some(FEED_INVALID));
    assert!(w.signals(p.id).is_empty());

    w.feed.respond(200, vec![b' '; 2 * 1024 * 1024 + 1]);
    w.poll_after(60).await;
    let page = w.state(p.id).unwrap().status_page.unwrap();
    assert_eq!(page.error.as_deref(), Some(FEED_INVALID));
}

#[tokio::test]
async fn turning_the_setting_off_mid_flight_discards_the_result_and_resuming_is_a_silent_baseline()
{
    let w = World::new().await;
    w.feed.respond_json(outage("major_outage"));
    let gate = Gate::new();
    w.feed.set_gate(Some(&gate));
    let p = w.provider(ComponentFilter::All).await;
    w.start().await;
    until("the held request", || w.feed.request_count() == 1).await;

    busy(w.h.config_state.update_general_settings(settings(false)))
        .await
        .unwrap();
    until("the status pages to be suspended", || {
        w.state(p.id)
            .is_some_and(|state| state.status_page.is_none())
    })
    .await;
    gate.open();
    quiesce().await;
    advance(600).await;
    quiesce().await;

    assert!(w.signals(p.id).is_empty());
    assert!(w.state(p.id).unwrap().status_page.is_none());
    assert_eq!(w.feed.request_count(), 1);
    assert!(w.alerts().is_empty());

    busy(w.h.config_state.update_general_settings(settings(true)))
        .await
        .unwrap();
    until("a request after resuming", || w.feed.request_count() == 2).await;
    until("the outage to show again", || !w.signals(p.id).is_empty()).await;
    quiesce().await;
    assert!(w.alerts().is_empty());

    w.feed.respond_json(statuspage_body(
        &[("web", "major_outage"), ("api", "operational")],
        json!([incident("inc1", "critical")]),
    ));
    w.next_poll().await;
    until("an alert for the new incident", || w.alerts().len() == 1).await;
}

#[tokio::test]
async fn an_outage_followed_by_the_setting_turned_off_never_recovers() {
    let w = World::new().await;
    let p = w.provider(ComponentFilter::All).await;
    w.start().await;
    w.first_poll().await;
    w.feed.respond_json(outage("partial_outage"));
    w.next_poll().await;
    until("the alert", || w.alerts().len() == 1).await;

    busy(w.h.config_state.update_general_settings(settings(false)))
        .await
        .unwrap();
    until("suspension", || {
        w.state(p.id)
            .is_some_and(|state| state.status_page.is_none())
    })
    .await;
    quiesce().await;

    assert_eq!(w.level(p.id), HealthLevel::Operational);
    assert!(w.signals(p.id).is_empty());
    assert_eq!(w.alerts().len(), 1);
    assert!(!w.alerts()[0].recovered);

    w.feed.respond_json(operational());
    busy(w.h.config_state.update_general_settings(settings(true)))
        .await
        .unwrap();
    until("a request after resuming", || w.feed.request_count() == 3).await;
    quiesce().await;
    w.next_poll().await;
    w.next_poll().await;

    assert_eq!(w.alerts().len(), 1);
}

#[tokio::test]
async fn a_redirect_from_https_to_loopback_http_is_refused() {
    let w = World::new().await;
    let target = FeedStub::start().await;
    w.feed.respond(302, "");
    w.feed.with_header("Location", target.feed_url().as_str());
    let p = w.provider(ComponentFilter::All).await;
    w.start().await;
    w.first_poll().await;

    let page = w.state(p.id).unwrap().status_page.unwrap();

    assert_eq!(target.request_count(), 0);
    assert_eq!(page.error.as_deref(), Some(FEED_UNREACHABLE));
    assert!(w.signals(p.id).is_empty());
}

#[tokio::test]
async fn stopping_cancels_a_request_in_flight_and_the_actor_never_returns() {
    let w = World::new().await;
    let gate = Gate::new();
    w.feed.set_gate(Some(&gate));
    let p = w.provider(ComponentFilter::All).await;
    w.start().await;
    w.service().start().await;
    until("the held request", || w.feed.request_count() == 1).await;
    quiesce().await;
    assert_eq!(w.feed.request_count(), 1);

    w.service().stop().await;
    gate.open();
    w.service().stop().await;
    w.service().start().await;
    w.outcome(&p, Some(FailureKind::Network), 2);
    advance(1000).await;
    quiesce().await;

    assert_eq!(w.feed.request_count(), 1);
    assert!(w.signals(p.id).is_empty());
}

#[tokio::test]
async fn two_providers_on_one_feed_share_a_request_and_keep_their_own_filters() {
    let w = World::new().await;
    w.feed.respond_json(outage("major_outage"));
    let watching_web = w
        .provider(ComponentFilter::Ids(vec!["web".to_owned()]))
        .await;
    let watching_api = w
        .provider(ComponentFilter::Ids(vec!["api".to_owned()]))
        .await;
    w.start().await;
    until("the outage for the first provider", || {
        !w.signals(watching_web.id).is_empty()
    })
    .await;
    quiesce().await;

    assert_eq!(w.feed.request_count(), 1);
    assert_eq!(w.level(watching_web.id), HealthLevel::MajorOutage);
    assert_eq!(w.level(watching_api.id), HealthLevel::Operational);
    assert!(w.signals(watching_api.id).is_empty());
    assert!(w.state(watching_web.id).unwrap().outage_floor);
    assert!(!w.state(watching_api.id).unwrap().outage_floor);
    assert_eq!(
        w.h.health.floors(secs(120)),
        HashMap::from([(watching_web.id, secs(120))])
    );
}

#[tokio::test]
async fn the_connection_signal_needs_two_failures_but_a_rate_limit_shows_at_once() {
    let w = World::new().await;
    let p = w.plain().await;
    let name =
        w.h.provider_service
            .get_provider_config(p.id)
            .await
            .unwrap()
            .display_name()
            .to_string();
    w.start().await;

    w.outcome(&p, Some(FailureKind::Network), 1);
    until("the provider to be known", || w.state(p.id).is_some()).await;
    quiesce().await;
    assert!(w.signals(p.id).is_empty());

    w.outcome(&p, Some(FailureKind::Network), 2);
    until("the connection signal", || !w.signals(p.id).is_empty()).await;
    let state = w.state(p.id).unwrap();
    let signal = &state.signals[0];
    assert_eq!(signal.kind, SignalKind::Connection);
    assert_eq!(signal.level, HealthLevel::MajorOutage);
    assert_eq!(signal.title, format!("Can't reach {name}"));
    assert!(!signal.episode_id.is_empty());
    assert!(state.outage_floor);
    let episode = signal.episode_id.clone();

    w.outcome(&p, Some(FailureKind::Timeout), 3);
    quiesce().await;
    assert_eq!(w.signals(p.id)[0].episode_id, episode);

    w.outcome(&p, None, 0);
    until("the signal to clear", || w.signals(p.id).is_empty()).await;
    assert!(!w.state(p.id).unwrap().outage_floor);

    w.outcome(&p, Some(FailureKind::RateLimited), 1);
    until("the rate limit", || !w.signals(p.id).is_empty()).await;
    let limited = w.signals(p.id).remove(0);
    assert_eq!(limited.kind, SignalKind::RateLimit);
    assert_eq!(limited.level, HealthLevel::Degraded);
    assert_eq!(limited.title, format!("Rate limited by {name}"));
    assert_ne!(limited.episode_id, episode);

    w.outcome(&p, None, 0);
    until("the rate limit to clear", || w.signals(p.id).is_empty()).await;
    w.outcome(&p, Some(FailureKind::Unavailable), 1);
    quiesce().await;
    assert!(w.signals(p.id).is_empty());
    w.outcome(&p, Some(FailureKind::Unavailable), 2);
    until("server errors", || !w.signals(p.id).is_empty()).await;
    assert_eq!(w.level(p.id), HealthLevel::PartialOutage);
    assert!(w.alerts().len() <= 2);
    assert!(w
        .alerts()
        .iter()
        .all(|alert| alert.level != HealthLevel::Degraded));
}

#[tokio::test]
async fn published_summaries_carry_the_health_the_actor_computed() {
    let w = World::new().await;
    let p = w.plain().await;
    w.start().await;

    w.outcome(&p, Some(FailureKind::Network), 2);
    until("a published summary with the signal", || {
        w.h.bus.events().iter().any(|event| {
            matches!(event, CoreEvent::ProviderStatusUpdated { provider }
                if provider.id == p.id && provider.health.signals.len() == 1)
        })
    })
    .await;
    let summaries = busy(w.h.provider_service.list_providers()).await.unwrap();

    assert_eq!(summaries[0].health.signals.len(), 1);
    assert_eq!(summaries[0].health.level, HealthLevel::MajorOutage);
    let published = w.h.bus.count("provider-status-updated");
    w.outcome(&p, Some(FailureKind::Network), 3);
    quiesce().await;
    assert_eq!(w.h.bus.count("provider-status-updated"), published);
}

#[tokio::test]
async fn an_outage_alerts_once_and_a_baseline_outage_never_does() {
    let w = World::new().await;
    w.feed.respond_json(outage("partial_outage"));
    let p = w.provider(ComponentFilter::All).await;
    w.start().await;
    w.first_poll().await;
    until("the banner", || w.level(p.id) == HealthLevel::PartialOutage).await;
    for _ in 0..3 {
        w.next_poll().await;
    }
    assert!(w.alerts().is_empty());

    w.feed.respond_json(operational());
    w.next_poll().await;
    w.next_poll().await;
    assert!(w.alerts().is_empty());

    w.feed.respond_json(outage("major_outage"));
    w.next_poll().await;
    until("the alert", || w.alerts().len() == 1).await;
    w.next_poll().await;
    w.next_poll().await;

    let alerts = w.alerts();
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].provider_ids, [p.id]);
    assert_eq!(alerts[0].level, HealthLevel::MajorOutage);
    assert!(!alerts[0].recovered);
    assert!(alerts[0].key.starts_with("status-page:"));
}

#[tokio::test]
async fn alert_links_follow_what_the_alert_is_about() {
    let w = World::new().await;
    let p = w.provider(ComponentFilter::All).await;
    w.start().await;
    w.first_poll().await;

    w.feed.respond_json(statuspage_body(
        &[("web", "major_outage"), ("api", "operational")],
        json!([incident("inc1", "critical")]),
    ));
    w.next_poll().await;
    until("the incident alert", || w.alerts().len() == 1).await;
    w.feed.respond_json(outage("major_outage"));
    w.next_poll().await;
    until("the source alert", || w.alerts().len() == 2).await;

    let alerts = w.alerts();
    assert_eq!(
        alerts[0].link,
        Some(AlertLink {
            url: "https://status.example.com/incidents/inc1".to_owned(),
            kind: AlertLinkKind::Incident,
        })
    );
    assert_eq!(alerts[0].title, "Incident inc1");
    assert_eq!(
        alerts[1].link,
        Some(AlertLink {
            url: "https://status.example.com/".to_owned(),
            kind: AlertLinkKind::StatusPage,
        })
    );
    assert_eq!(alerts[0].key, alerts[1].key);
    assert_ne!(alerts[0].alert_id, alerts[1].alert_id);
    assert_eq!(alerts[1].provider_ids, [p.id]);
}

#[tokio::test]
async fn a_recovery_needs_an_alert_and_two_calm_checks() {
    let w = World::new().await;
    let p = w.provider(ComponentFilter::All).await;
    w.start().await;
    w.first_poll().await;
    w.feed.respond_json(outage("partial_outage"));
    w.next_poll().await;
    until("the alert", || w.alerts().len() == 1).await;

    w.feed.respond_json(operational());
    w.next_poll().await;
    assert_eq!(w.alerts().len(), 1);
    w.feed.respond_json(outage("partial_outage"));
    w.next_poll().await;
    w.feed.respond_json(operational());
    w.next_poll().await;
    assert_eq!(w.alerts().len(), 1);
    w.next_poll().await;
    until("the recovery", || w.alerts().len() == 2).await;
    w.next_poll().await;
    w.next_poll().await;

    let alerts = w.alerts();
    assert_eq!(alerts.len(), 2);
    assert!(alerts[1].recovered);
    assert_eq!(alerts[1].key, alerts[0].key);
    assert_eq!(alerts[1].provider_ids, [p.id]);
    assert_eq!(w.level(p.id), HealthLevel::Operational);
}

#[tokio::test]
async fn a_recurrence_inside_the_cooldown_waits_for_it() {
    let w = World::new().await;
    w.provider(ComponentFilter::All).await;
    w.start().await;
    w.first_poll().await;
    w.feed.respond_json(outage("major_outage"));
    w.next_poll().await;
    until("the first alert", || w.alerts().len() == 1).await;
    w.feed.respond_json(operational());
    w.next_poll().await;
    w.next_poll().await;
    until("the recovery", || w.alerts().len() == 2).await;

    w.feed.respond_json(outage("major_outage"));
    for _ in 0..5 {
        w.next_poll().await;
    }
    assert_eq!(w.alerts().len(), 2);

    w.next_poll().await;
    until("the second alert", || w.alerts().len() == 3).await;
    let alerts = w.alerts();
    assert!(!alerts[2].recovered);
    assert_ne!(alerts[2].alert_id, alerts[0].alert_id);
}

#[tokio::test]
async fn a_recurrence_after_the_cooldown_alerts_at_once() {
    let w = World::new().await;
    w.provider(ComponentFilter::All).await;
    w.start().await;
    w.first_poll().await;
    w.feed.respond_json(outage("major_outage"));
    w.next_poll().await;
    w.feed.respond_json(operational());
    w.next_poll().await;
    w.next_poll().await;
    until("the recovery", || w.alerts().len() == 2).await;
    for _ in 0..7 {
        w.next_poll().await;
    }

    w.feed.respond_json(outage("major_outage"));
    w.next_poll().await;

    until("the second alert", || w.alerts().len() == 3).await;
}

#[tokio::test]
async fn a_feed_that_stops_answering_goes_stale_then_disappears_without_a_recovery() {
    let w = World::new().await;
    let p = w.provider(ComponentFilter::All).await;
    w.h.start().await;
    w.start().await;
    w.first_poll().await;
    w.feed.respond_json(outage("major_outage"));
    w.next_poll().await;
    until("the alert", || w.alerts().len() == 1).await;
    assert!(w.state(p.id).unwrap().outage_floor);
    w.feed.respond(500, "down");

    advance(898).await;
    quiesce().await;
    assert_eq!(w.signals(p.id)[0].evidence, EvidenceView::Fresh);
    assert!(w.state(p.id).unwrap().outage_floor);
    advance(2).await;
    until("the signal to go stale", || w.stale(p.id)).await;
    quiesce().await;

    let state = w.state(p.id).unwrap();
    assert_eq!(state.level, HealthLevel::Operational);
    assert!(!state.outage_floor);
    assert!(w.h.health.floors(secs(120)).is_empty());
    assert_eq!(state.signals.len(), 1);
    assert_eq!(state.signals[0].level, HealthLevel::MajorOutage);
    assert!(state.status_page.unwrap().error.is_some());

    advance(2700).await;
    until("the stale signal to be dropped", || {
        w.signals(p.id).is_empty()
    })
    .await;
    quiesce().await;
    assert_eq!(w.alerts().len(), 1);
    assert!(!w.alerts()[0].recovered);
    assert_eq!(p.stub.fetch_count(), 0);
}

#[tokio::test]
async fn a_fresh_check_after_a_gap_still_needs_a_second_one_to_recover() {
    let w = World::new().await;
    let p = w.provider(ComponentFilter::All).await;
    w.h.start().await;
    w.start().await;
    w.first_poll().await;
    w.feed.respond_json(outage("major_outage"));
    w.next_poll().await;
    until("the alert", || w.alerts().len() == 1).await;
    w.feed.respond(500, "down");

    advance(900).await;
    until("the signal to go stale", || w.stale(p.id)).await;
    quiesce().await;
    w.feed.respond_json(operational());
    w.poll_after(60).await;
    until("the signal to clear", || w.signals(p.id).is_empty()).await;
    assert_eq!(w.alerts().len(), 1);

    w.next_poll().await;
    until("the recovery", || w.alerts().len() == 2).await;
    w.next_poll().await;

    assert!(w.alerts()[1].recovered);
    assert_eq!(w.alerts().len(), 2);
    assert_eq!(p.stub.fetch_count(), 0);
}

#[tokio::test]
async fn removing_the_source_during_an_outage_is_silent() {
    let w = World::new().await;
    let p = w.provider(ComponentFilter::All).await;
    w.start().await;
    w.first_poll().await;
    w.feed.respond_json(outage("major_outage"));
    w.next_poll().await;
    until("the alert", || w.alerts().len() == 1).await;

    w.h.stubs.queue(StubProvider::new(p.id));
    let mut edited = w.h.config("renamed", 0);
    edited.id = Some(p.id);
    busy(w.h.provider_service.update_provider(p.id, edited.clone()))
        .await
        .unwrap();
    until("the source to disappear", || {
        w.state(p.id)
            .is_some_and(|state| state.status_page.is_none() && state.signals.is_empty())
    })
    .await;
    quiesce().await;
    advance(600).await;
    quiesce().await;
    assert_eq!(w.alerts().len(), 1);
    assert_eq!(w.level(p.id), HealthLevel::Operational);
    let requests = w.feed.request_count();

    w.h.stubs
        .queue(StubProvider::new(p.id).with_source(w.source(ComponentFilter::All)));
    busy(w.h.provider_service.update_provider(p.id, edited))
        .await
        .unwrap();
    until("a request for the restored source", || {
        w.feed.request_count() > requests
    })
    .await;
    until("the outage to show again", || !w.signals(p.id).is_empty()).await;
    quiesce().await;

    assert_eq!(w.alerts().len(), 1);
    assert!(w.alerts().iter().all(|alert| !alert.recovered));
}

#[tokio::test]
async fn recovery_fetches_each_current_provider_once_and_only_after_a_calm_observation() {
    let w = World::new().await;
    w.feed.respond_json(outage("major_outage"));
    let first = w.provider(ComponentFilter::All).await;
    let second = w.provider(ComponentFilter::All).await;
    let removed = w.provider(ComponentFilter::All).await;
    w.h.start().await;
    w.start().await;
    w.first_poll().await;
    w.next_poll().await;
    assert_eq!(first.stub.fetch_count() + second.stub.fetch_count(), 0);
    busy(w.h.provider_service.remove_provider(removed.id))
        .await
        .unwrap();
    quiesce().await;

    w.feed.respond_json(operational());
    w.next_poll().await;
    until("both providers to be fetched", || {
        first.stub.fetch_count() == 1 && second.stub.fetch_count() == 1
    })
    .await;
    quiesce().await;
    w.next_poll().await;
    w.next_poll().await;

    assert_eq!(first.stub.fetch_count(), 1);
    assert_eq!(second.stub.fetch_count(), 1);
    assert_eq!(removed.stub.fetch_count(), 0);
}

#[tokio::test]
async fn a_feed_that_keeps_failing_triggers_no_recovery_fetch() {
    let w = World::new().await;
    let p = w.provider(ComponentFilter::All).await;
    w.h.start().await;
    w.start().await;
    w.first_poll().await;
    w.feed.respond_json(outage("major_outage"));
    w.next_poll().await;
    w.feed.respond(500, "down");
    for _ in 0..4 {
        w.poll_after(1800).await;
    }

    assert_eq!(p.stub.fetch_count(), 0);
}

#[tokio::test]
async fn a_connection_outage_recovers_after_exactly_one_committed_success() {
    let w = World::new().await;
    let p = w.plain().await;
    w.start().await;

    w.outcome(&p, Some(FailureKind::Network), 1);
    w.outcome(&p, Some(FailureKind::Network), 2);
    until("the alert", || w.alerts().len() == 1).await;
    w.outcome(&p, Some(FailureKind::Network), 3);
    quiesce().await;
    w.h.refresh_manager.set_mode(RefreshMode::Idle);
    advance(900).await;
    quiesce().await;
    w.h.health.send(HealthInput::FetchOutcome {
        provider_id: p.id,
        generation: Generation::next(),
        view: ScheduleView {
            failures: 0,
            last_failure: None,
            next_attempt_in: Duration::ZERO,
            in_flight: false,
        },
        kind: None,
    });
    quiesce().await;
    assert_eq!(w.alerts().len(), 1);
    assert_eq!(w.signals(p.id).len(), 1);

    w.outcome(&p, None, 0);
    until("the recovery", || w.alerts().len() == 2).await;
    until("the signal to clear", || w.signals(p.id).is_empty()).await;
    w.outcome(&p, None, 0);
    quiesce().await;

    let alerts = w.alerts();
    assert_eq!(alerts.len(), 2);
    assert!(!alerts[0].recovered && alerts[1].recovered);
    assert_eq!(alerts[0].link, None);
    assert_eq!(alerts[0].key, format!("connection:{}", p.id));
    assert_eq!(alerts[1].key, alerts[0].key);
    assert!(alerts[1].title.ends_with("is reachable again"));
    assert_eq!(alerts[0].provider_ids, [p.id]);
    assert_eq!(alerts[1].provider_ids, [p.id]);
}

#[tokio::test]
async fn a_non_transient_error_replaces_the_connection_signal_without_a_recovery() {
    let w = World::new().await;
    let p = w.plain().await;
    w.start().await;
    w.outcome(&p, Some(FailureKind::Timeout), 1);
    w.outcome(&p, Some(FailureKind::Timeout), 2);
    until("the alert", || w.alerts().len() == 1).await;

    w.outcome(&p, Some(FailureKind::Auth), 2);
    until("the signal to be replaced", || w.signals(p.id).is_empty()).await;
    for _ in 0..3 {
        w.outcome(&p, None, 0);
    }
    quiesce().await;

    assert_eq!(w.alerts().len(), 1);
    assert!(!w.alerts()[0].recovered);

    assert_eq!(w.alerts().len(), 1);
    advance(900).await;
    quiesce().await;
    w.outcome(&p, Some(FailureKind::Network), 2);
    until("a new episode to alert", || w.alerts().len() == 2).await;
}

#[tokio::test]
async fn controller_checks_run_every_two_minutes_after_a_successful_fetch_only() {
    let w = World::new().await;
    let p = w.instance_provider(|stub| stub).await;
    w.start().await;
    advance(600).await;
    quiesce().await;
    assert_eq!(p.stub.health_check_count(), 0);

    w.outcome(&p, None, 0);
    until("the first check", || p.stub.health_check_count() == 1).await;
    quiesce().await;
    advance(119).await;
    quiesce().await;
    assert_eq!(p.stub.health_check_count(), 1);
    advance(1).await;
    until("the second check", || p.stub.health_check_count() == 2).await;
    quiesce().await;

    w.outcome(&p, Some(FailureKind::Network), 1);
    quiesce().await;
    advance(600).await;
    quiesce().await;
    assert_eq!(p.stub.health_check_count(), 2);

    w.h.refresh_manager.set_mode(RefreshMode::Idle);
    quiesce().await;
    w.outcome(&p, None, 0);
    quiesce().await;
    advance(600).await;
    quiesce().await;
    assert_eq!(p.stub.health_check_count(), 2);
}

#[tokio::test]
async fn controller_checks_run_four_at_a_time_and_time_out_after_ten_seconds() {
    let w = World::new().await;
    let gate = Gate::new();
    let mut providers = Vec::new();
    for _ in 0..6 {
        providers.push(w.instance_provider(|stub| stub.health_gated(&gate)).await);
    }
    w.start().await;
    for p in &providers {
        w.outcome(p, None, 0);
    }
    let started = || {
        providers
            .iter()
            .map(|p| p.stub.health_check_count())
            .sum::<usize>()
    };

    until("four checks", || started() == 4).await;
    quiesce().await;
    assert_eq!(started(), 4);
    gate.open();
    until("the rest", || started() == 6).await;
    quiesce().await;

    let hanging = Gate::new();
    let slow = busy(w.instance_provider(|stub| stub.health_gated(&hanging))).await;
    w.outcome(&slow, None, 0);
    until("the hanging check", || slow.stub.health_check_count() == 1).await;
    advance(10).await;
    quiesce().await;
    advance(120).await;
    until("a new check after the timeout", || {
        slow.stub.health_check_count() == 2
    })
    .await;
}

#[tokio::test]
async fn a_controller_outage_alerts_and_losing_access_never_recovers_it() {
    let w = World::new().await;
    let p = w.instance_provider(|stub| stub).await;
    w.start().await;
    w.outcome(&p, None, 0);
    until("the baseline check", || p.stub.health_check_count() == 1).await;
    quiesce().await;
    assert!(w.alerts().is_empty());

    let broken = ComponentHealth {
        name: "kustomize-controller".to_owned(),
        level: HealthLevel::MajorOutage,
        detail: Some("scaled to zero".to_owned()),
    };
    p.stub.set_health(Ok(vec![broken.clone()]));
    advance(120).await;
    until("the controller alert", || w.alerts().len() == 1).await;
    quiesce().await;
    let signal = w.signals(p.id).remove(0);
    assert_eq!(signal.kind, SignalKind::Instance);
    assert_eq!(signal.title, "kustomize-controller: scaled to zero");
    assert_eq!(w.level(p.id), HealthLevel::MajorOutage);
    let alert = w.alerts().remove(0);
    assert_eq!(alert.key, format!("instance:{}", p.id));
    assert_eq!(alert.link, None);
    assert_eq!(alert.provider_ids, [p.id]);

    p.stub
        .set_health(Err(DomainError::PermissionDenied("forbidden".into())));
    advance(120).await;
    until("the denied check", || p.stub.health_check_count() == 3).await;
    until("the signal to disappear", || w.signals(p.id).is_empty()).await;
    quiesce().await;
    assert_eq!(w.level(p.id), HealthLevel::Operational);
    p.stub
        .set_health(Err(DomainError::PipelineNotFound("missing".into())));
    advance(120).await;
    until("the missing controller", || {
        p.stub.health_check_count() == 4
    })
    .await;
    quiesce().await;
    assert!(w.signals(p.id).is_empty());
    assert_eq!(w.alerts().len(), 1);

    p.stub.set_health(Ok(Vec::new()));
    advance(120).await;
    until("the first calm check", || p.stub.health_check_count() == 5).await;
    quiesce().await;
    assert_eq!(w.alerts().len(), 1);
    advance(120).await;
    until("the recovery", || w.alerts().len() == 2).await;
    assert!(w.alerts()[1].recovered);
}

#[tokio::test]
async fn a_failing_controller_check_only_ages_the_evidence() {
    let w = World::new().await;
    let p = w.instance_provider(|stub| stub).await;
    p.stub.set_health(Ok(vec![ComponentHealth {
        name: "helm-controller".to_owned(),
        level: HealthLevel::Degraded,
        detail: Some("1/2 replicas available".to_owned()),
    }]));
    w.start().await;
    w.outcome(&p, None, 0);
    until("the signal", || !w.signals(p.id).is_empty()).await;
    quiesce().await;
    assert_eq!(w.level(p.id), HealthLevel::Degraded);
    assert!(w.alerts().is_empty());

    p.stub
        .set_health(Err(DomainError::NetworkError("down".into())));
    advance(600).await;
    quiesce().await;
    assert_eq!(w.signals(p.id)[0].evidence, EvidenceView::Fresh);
    advance(300).await;
    until("the signal to go stale", || w.stale(p.id)).await;
    quiesce().await;

    assert_eq!(w.level(p.id), HealthLevel::Operational);
    assert_eq!(w.signals(p.id)[0].level, HealthLevel::Degraded);
}

#[tokio::test]
async fn results_for_an_old_generation_are_ignored() {
    let w = World::new().await;
    let p = w.plain().await;
    w.start().await;
    until("the provider to be known", || w.state(p.id).is_some()).await;
    let newer = Generation::next();
    w.h.health.send(HealthInput::ProvidersChanged {
        changed: vec![ProviderHealthInput {
            id: p.id,
            generation: Some(newer),
            display_name: "renamed".to_owned(),
            source: None,
            has_instance_checks: false,
        }],
    });

    w.outcome(&p, Some(FailureKind::Network), 2);
    w.h.health.send(HealthInput::FetchOutcome {
        provider_id: p.id,
        generation: newer,
        view: ScheduleView {
            failures: 1,
            last_failure: Some(FailureKind::RateLimited),
            next_attempt_in: Duration::ZERO,
            in_flight: false,
        },
        kind: Some(FailureKind::RateLimited),
    });
    until("the newer outcome", || !w.signals(p.id).is_empty()).await;
    quiesce().await;

    let state = w.state(p.id).unwrap();
    assert_eq!(state.generation, newer);
    assert_eq!(state.signals.len(), 1);
    assert_eq!(state.signals[0].kind, SignalKind::RateLimit);
    assert!(w.alerts().is_empty());
}

#[tokio::test]
async fn a_controller_result_for_an_old_generation_is_ignored() {
    let w = World::new().await;
    let gate = Gate::new();
    let p = w.instance_provider(|stub| stub.health_gated(&gate)).await;
    p.stub.set_health(Ok(vec![ComponentHealth {
        name: "source-controller".to_owned(),
        level: HealthLevel::MajorOutage,
        detail: None,
    }]));
    w.start().await;
    w.outcome(&p, None, 0);
    until("the held check", || p.stub.health_check_count() == 1).await;
    w.h.health.send(HealthInput::ProvidersChanged {
        changed: vec![ProviderHealthInput {
            id: p.id,
            generation: Some(Generation::next()),
            display_name: "renamed".to_owned(),
            source: None,
            has_instance_checks: true,
        }],
    });
    quiesce().await;

    gate.open();
    quiesce().await;

    assert!(w.signals(p.id).is_empty());
    assert!(w.alerts().is_empty());
}

#[tokio::test]
async fn an_outcome_queued_before_an_update_publishes_nothing() {
    let w = World::new().await;
    let p = w.plain().await;
    w.outcome(&p, Some(FailureKind::Network), 2);
    let mut edited = w.h.config("renamed", 0);
    edited.id = Some(p.id);
    w.h.stubs.queue(StubProvider::new(p.id));
    busy(w.h.provider_service.update_provider(p.id, edited))
        .await
        .unwrap();
    let updated_at = w.h.bus.events().len();
    let newer = w.h.provider_service.acquire(p.id).await.unwrap().generation;

    w.start().await;
    until("the actor to reach the new generation", || {
        w.state(p.id).is_some_and(|state| state.generation == newer)
    })
    .await;
    quiesce().await;

    let after: Vec<&'static str> = w.h.bus.names().split_off(updated_at);
    assert!(
        !after.contains(&"provider-status-updated"),
        "published after the update: {after:?}"
    );
    assert!(w.signals(p.id).is_empty());
}

#[tokio::test]
async fn alerts_only_name_providers_whose_generation_is_current() {
    let w = World::new().await;
    w.feed.respond_json(operational());
    let current = w.provider(ComponentFilter::All).await;
    let stale = w.provider(ComponentFilter::All).await;
    w.h.health.send(HealthInput::ProvidersChanged {
        changed: vec![ProviderHealthInput {
            id: stale.id,
            generation: Some(Generation::next()),
            display_name: "ghost".to_owned(),
            source: Some(w.source(ComponentFilter::All)),
            has_instance_checks: false,
        }],
    });
    w.start().await;
    w.first_poll().await;
    w.feed.respond_json(outage("major_outage"));
    w.next_poll().await;
    until("the alert", || !w.alerts().is_empty()).await;

    let alerts = w.alerts();

    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].provider_ids, [current.id]);
}

#[tokio::test]
async fn starting_twice_runs_one_actor_and_a_stopped_one_never_comes_back() {
    let w = World::new().await;
    w.provider(ComponentFilter::All).await;
    w.start().await;
    w.service().start().await;
    w.first_poll().await;
    advance(1).await;
    quiesce().await;
    assert_eq!(w.feed.request_count(), 1);

    w.service().stop().await;
    w.service().start().await;
    advance(1000).await;
    quiesce().await;

    assert_eq!(w.feed.request_count(), 1);
}

#[tokio::test]
async fn every_loaded_slot_is_announced_to_the_actor() {
    let w = World::new().await;
    let first = w.provider(ComponentFilter::All).await;
    let second = w.plain().await;

    let mut inputs = w.h.provider_service.health_inputs().await;
    inputs.sort_by_key(|input| input.id);

    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[0].id, first.id);
    assert_eq!(inputs[0].generation, Some(first.generation));
    assert!(inputs[0].source.is_some());
    assert_eq!(inputs[1].id, second.id);
    assert_eq!(inputs[1].generation, Some(second.generation));
    assert!(inputs[1].source.is_none());
}
