use chrono::{
    TimeZone,
    Utc,
};
use pipedash_plugin_api::{
    ComponentFilter,
    ComponentHealth,
    FeedFormat,
    StatusSource,
};
use reqwest::Url;
use serde_json::json;

use super::*;
use crate::application::services::health_service::alert_candidates::{
    connection_candidate,
    connection_recovery,
    feed_candidates,
    feed_recovery,
    instance_candidates,
    instance_recovery,
    merge,
};
use crate::event::AlertLinkKind;
use crate::infrastructure::status_feeds::{
    evaluate,
    parse_statusio,
    parse_statuspage,
    FeedSignal,
};

const SUBJECT: &str = "status-page:test";

fn minutes(minutes: u64) -> Duration {
    Duration::from_secs(minutes * 60)
}

fn draft(title: &str) -> Draft {
    Draft {
        level: HealthLevel::PartialOutage,
        title: title.to_owned(),
        body: String::new(),
        link: None,
    }
}

fn outage(keys: &[&str], providers: &[i64]) -> Observation {
    Observation {
        candidates: keys
            .iter()
            .map(|key| Candidate {
                key: (*key).to_owned(),
                draft: draft(key),
                provider_ids: providers.to_vec(),
            })
            .collect(),
        recovery: Draft {
            level: HealthLevel::Operational,
            title: "recovered".to_owned(),
            body: String::new(),
            link: None,
        },
        provider_ids: providers.to_vec(),
        calm_needed: CALM_CHECKS,
    }
}

fn healthy() -> Observation {
    outage(&[], &[1])
}

#[test]
fn the_first_observation_is_a_silent_baseline() {
    let mut tracker = AlertTracker::default();
    let now = Instant::now();

    assert!(tracker
        .observe(SUBJECT, now, outage(&["a"], &[1]))
        .is_empty());
    assert!(tracker
        .observe(SUBJECT, now, outage(&["a"], &[1]))
        .is_empty());
    assert!(tracker.observe(SUBJECT, now, healthy()).is_empty());
    assert!(tracker.observe(SUBJECT, now, healthy()).is_empty());
}

#[test]
fn an_alert_fires_once_per_key_and_a_new_key_alerts_again() {
    let mut tracker = AlertTracker::default();
    let now = Instant::now();
    tracker.observe(SUBJECT, now, healthy());

    let first = tracker.observe(SUBJECT, now, outage(&["a"], &[1, 2]));
    let repeated = tracker.observe(SUBJECT, now, outage(&["a"], &[1, 2]));
    let second = tracker.observe(SUBJECT, now, outage(&["a", "b"], &[1, 2]));

    assert_eq!(first.len(), 1);
    assert_eq!(first[0].provider_ids, [1, 2]);
    assert_eq!(first[0].notification_key, SUBJECT);
    assert!(!first[0].recovered);
    assert!(repeated.is_empty());
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].title, "b");
    assert_ne!(first[0].alert_id, second[0].alert_id);
}

#[test]
fn recovery_needs_two_consecutive_observations_after_an_alert() {
    let mut tracker = AlertTracker::default();
    let now = Instant::now();
    tracker.observe(SUBJECT, now, healthy());
    tracker.observe(SUBJECT, now, outage(&["a"], &[1]));

    assert!(tracker.observe(SUBJECT, now, healthy()).is_empty());
    let relapse = tracker.observe(SUBJECT, now, outage(&["a"], &[1]));
    assert!(relapse.is_empty());
    assert!(tracker.observe(SUBJECT, now, healthy()).is_empty());
    let recovered = tracker.observe(SUBJECT, now, healthy());

    assert_eq!(recovered.len(), 1);
    assert!(recovered[0].recovered);
    assert_eq!(recovered[0].title, "recovered");
    assert_eq!(recovered[0].notification_key, SUBJECT);
    assert!(tracker.observe(SUBJECT, now, healthy()).is_empty());
    assert!(tracker.observe(SUBJECT, now, healthy()).is_empty());
}

#[test]
fn a_connection_style_observation_recovers_after_one_calm_fetch() {
    let mut tracker = AlertTracker::default();
    let now = Instant::now();
    let calm = || Observation {
        calm_needed: CALM_FETCHES,
        ..healthy()
    };
    tracker.reset(SUBJECT);
    assert_eq!(
        tracker
            .observe(SUBJECT, now, outage(&["connection"], &[1]))
            .len(),
        1
    );

    let recovered = tracker.observe(SUBJECT, now, calm());

    assert_eq!(recovered.len(), 1);
    assert!(recovered[0].recovered);
    assert!(tracker.observe(SUBJECT, now, calm()).is_empty());
}

#[test]
fn a_gap_resets_the_recovery_count() {
    let mut tracker = AlertTracker::default();
    let now = Instant::now();
    tracker.observe(SUBJECT, now, healthy());
    tracker.observe(SUBJECT, now, outage(&["a"], &[1]));

    assert!(tracker.observe(SUBJECT, now, healthy()).is_empty());
    tracker.gap(SUBJECT);
    assert!(tracker.observe(SUBJECT, now, healthy()).is_empty());
    let recovered = tracker.observe(SUBJECT, now, healthy());

    assert_eq!(recovered.len(), 1);
    assert!(recovered[0].recovered);
}

#[test]
fn a_gap_alone_never_recovers_and_a_closed_subject_forgets_everything() {
    let mut tracker = AlertTracker::default();
    let now = Instant::now();
    tracker.observe(SUBJECT, now, healthy());
    tracker.observe(SUBJECT, now, outage(&["a"], &[1]));

    tracker.gap(SUBJECT);
    tracker.gap(SUBJECT);
    tracker.close(SUBJECT);

    assert!(tracker
        .observe(SUBJECT, now, outage(&["a"], &[1]))
        .is_empty());
    assert!(tracker.observe(SUBJECT, now, healthy()).is_empty());
    assert!(tracker.observe(SUBJECT, now, healthy()).is_empty());
}

#[test]
fn a_reset_subject_alerts_on_its_first_outage() {
    let mut tracker = AlertTracker::default();
    let now = Instant::now();
    tracker.reset(SUBJECT);

    let alerts = tracker.observe(SUBJECT, now, outage(&["connection"], &[1]));

    assert_eq!(alerts.len(), 1);
}

#[test]
fn recovery_is_only_for_keys_that_alerted() {
    let mut tracker = AlertTracker::default();
    let now = Instant::now();
    tracker.observe(SUBJECT, now, outage(&["a"], &[1]));

    for _ in 0..4 {
        assert!(tracker.observe(SUBJECT, now, healthy()).is_empty());
    }
}

#[test]
fn a_recurrence_inside_the_cooldown_waits_and_one_after_it_alerts() {
    let mut tracker = AlertTracker::default();
    let start = Instant::now();
    tracker.observe(SUBJECT, start, healthy());
    assert_eq!(
        tracker.observe(SUBJECT, start, outage(&["a"], &[1])).len(),
        1
    );
    tracker.observe(SUBJECT, start + minutes(2), healthy());
    let recovered = tracker.observe(SUBJECT, start + minutes(4), healthy());
    assert_eq!(recovered.len(), 1);

    let early = tracker.observe(SUBJECT, start + minutes(6), outage(&["a"], &[1]));
    let still_early = tracker.observe(SUBJECT, start + minutes(10), outage(&["a"], &[1]));
    let after = tracker.observe(SUBJECT, start + minutes(16), outage(&["a"], &[1]));
    let later = tracker.observe(SUBJECT, start + minutes(18), outage(&["a"], &[1]));

    assert!(early.is_empty() && still_early.is_empty());
    assert_eq!(after.len(), 1);
    assert!(!after[0].recovered);
    assert!(later.is_empty());
}

#[test]
fn a_recurrence_after_the_cooldown_alerts_at_once() {
    let mut tracker = AlertTracker::default();
    let start = Instant::now();
    tracker.observe(SUBJECT, start, healthy());
    tracker.observe(SUBJECT, start, outage(&["a"], &[1]));
    tracker.observe(SUBJECT, start + minutes(2), healthy());
    tracker.observe(SUBJECT, start + minutes(4), healthy());

    let again = tracker.observe(SUBJECT, start + minutes(20), outage(&["a"], &[1]));

    assert_eq!(again.len(), 1);
}

fn reloaded(tracker: &AlertTracker) -> AlertTracker {
    serde_json::from_value(serde_json::to_value(tracker).unwrap()).unwrap()
}

#[test]
fn the_cooldown_survives_a_reload() {
    let mut tracker = AlertTracker::default();
    let start = Instant::now();
    tracker.observe(SUBJECT, start, healthy());
    assert_eq!(
        tracker.observe(SUBJECT, start, outage(&["a"], &[1])).len(),
        1
    );
    tracker.observe(SUBJECT, start, healthy());
    assert_eq!(tracker.observe(SUBJECT, start, healthy()).len(), 1);

    let mut tracker = reloaded(&tracker);
    let now = Instant::now();
    let early = tracker.observe(SUBJECT, now + minutes(2), outage(&["a"], &[1]));
    let after = tracker.observe(SUBJECT, now + minutes(16), outage(&["a"], &[1]));

    assert!(early.is_empty());
    assert_eq!(after.len(), 1);
    assert!(!after[0].recovered);
}

#[test]
fn an_expired_cooldown_is_not_stored() {
    let mut tracker = AlertTracker::default();
    let start = Instant::now();
    tracker.observe(SUBJECT, start, healthy());
    tracker.observe(SUBJECT, start, outage(&["a"], &[1]));
    tracker.observe(SUBJECT, start + minutes(20), healthy());

    let stored = serde_json::to_value(&tracker).unwrap();
    let mut tracker = reloaded(&tracker);

    assert_eq!(stored["alerted"], json!([]));
    assert_eq!(
        tracker
            .observe(SUBJECT, Instant::now(), outage(&["a"], &[1]))
            .len(),
        1
    );
}

#[test]
fn a_suppressed_recurrence_never_recovers() {
    let mut tracker = AlertTracker::default();
    let start = Instant::now();
    tracker.observe(SUBJECT, start, healthy());
    tracker.observe(SUBJECT, start, outage(&["a"], &[1]));
    tracker.observe(SUBJECT, start + minutes(2), healthy());
    tracker.observe(SUBJECT, start + minutes(3), healthy());
    tracker.observe(SUBJECT, start + minutes(5), outage(&["a"], &[1]));

    assert!(tracker
        .observe(SUBJECT, start + minutes(6), healthy())
        .is_empty());
    assert!(tracker
        .observe(SUBJECT, start + minutes(7), healthy())
        .is_empty());
}

#[test]
fn a_resolved_key_keeps_the_notification_until_everything_is_calm() {
    let mut tracker = AlertTracker::default();
    let now = Instant::now();
    tracker.observe(SUBJECT, now, healthy());
    tracker.observe(SUBJECT, now, outage(&["a"], &[1]));

    let replacement = tracker.observe(SUBJECT, now, outage(&["b"], &[1]));
    assert_eq!(replacement.len(), 1);
    assert!(tracker.observe(SUBJECT, now, healthy()).is_empty());
    let recovered = tracker.observe(SUBJECT, now, healthy());

    assert_eq!(recovered.len(), 1);
    assert!(recovered[0].recovered);
}

fn source(page: &str, format: FeedFormat) -> StatusSource {
    StatusSource {
        name: "Example".to_owned(),
        page_url: Url::parse(page).unwrap(),
        feed_url: Url::parse("http://127.0.0.1:1/api/v2/summary.json").unwrap(),
        format,
        components: ComponentFilter::All,
    }
}

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 22, 12, 0, 0).unwrap()
}

fn statuspage(components: serde_json::Value, incidents: serde_json::Value) -> FeedSignal {
    let body = json!({ "components": components, "incidents": incidents });
    let feed = parse_statuspage(body.to_string().as_bytes()).unwrap();

    evaluate(
        &feed,
        &ComponentFilter::All,
        &Url::parse("https://status.example.com").unwrap(),
        now(),
    )
}

#[test]
fn an_incident_alert_links_to_the_incident() {
    let signal = statuspage(
        json!([{"id": "c1", "name": "API", "status": "operational"}]),
        json!([{
            "id": "inc1", "name": "Builds are failing", "status": "investigating",
            "impact": "major", "updated_at": "2026-10-22T11:00:00Z",
            "components": [{"id": "c1"}],
            "incident_updates": [{"body": "We are looking into it", "created_at": "2026-10-22T11:00:00Z"}]
        }]),
    );

    let candidates = feed_candidates(
        &source("https://status.example.com", FeedFormat::Statuspage),
        &signal,
        7,
    );

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].key, "incident:inc1");
    assert_eq!(candidates[0].draft.title, "Example: Builds are failing");
    assert_eq!(candidates[0].draft.body, "We are looking into it");
    assert_eq!(candidates[0].provider_ids, [7]);
    assert_eq!(
        candidates[0].draft.link,
        Some(AlertLink {
            url: "https://status.example.com/incidents/inc1".to_owned(),
            kind: AlertLinkKind::Incident,
        })
    );
}

#[test]
fn a_source_level_alert_links_to_the_status_page() {
    let signal = statuspage(
        json!([{"id": "c1", "name": "API", "status": "major_outage"}]),
        json!([]),
    );

    let candidates = feed_candidates(
        &source("https://status.example.com", FeedFormat::Statuspage),
        &signal,
        7,
    );
    let recovery = feed_recovery(&source(
        "https://status.example.com",
        FeedFormat::Statuspage,
    ));

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].key, "source");
    assert_eq!(candidates[0].draft.level, HealthLevel::MajorOutage);
    assert_eq!(candidates[0].draft.title, "Example reports a major outage");
    assert_eq!(candidates[0].draft.body, "API: a major outage");
    let page = Some(AlertLink {
        url: "https://status.example.com/".to_owned(),
        kind: AlertLinkKind::StatusPage,
    });
    assert_eq!(candidates[0].draft.link, page);
    assert_eq!(recovery.link, page);
}

#[test]
fn a_status_io_incident_has_no_page_of_its_own_so_it_links_to_the_status_page() {
    let body = json!({
        "result": {
            "status_overall": {"status_code": 100},
            "status": [{"id": "c1", "name": "API", "status_code": 100, "containers": []}],
            "incidents": [{
                "_id": "inc1", "name": "Outage",
                "messages": [{"details": "Down", "state": 100, "status": 500, "datetime": "2026-10-22T11:00:00Z"}],
                "components_affected": [{"_id": "c1", "name": "API"}]
            }],
            "maintenance": {"active": [], "upcoming": []}
        }
    });
    let feed = parse_statusio(body.to_string().as_bytes()).unwrap();
    let page = Url::parse("https://status.example.com").unwrap();
    let signal = evaluate(&feed, &ComponentFilter::All, &page, now());

    let candidates = feed_candidates(
        &source("https://status.example.com", FeedFormat::StatusIo),
        &signal,
        1,
    );

    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].draft.link.as_ref().map(|link| link.kind),
        Some(AlertLinkKind::StatusPage)
    );
}

#[test]
fn degraded_and_maintenance_alone_never_become_candidates() {
    let degraded = statuspage(
        json!([{"id": "c1", "name": "API", "status": "degraded_performance"}]),
        json!([{
            "id": "inc1", "name": "Slow", "status": "investigating", "impact": "minor",
            "components": [{"id": "c1"}]
        }]),
    );
    let maintenance = statuspage(
        json!([{"id": "c1", "name": "API", "status": "under_maintenance"}]),
        json!([]),
    );
    let source = source("https://status.example.com", FeedFormat::Statuspage);

    assert!(feed_candidates(&source, &degraded, 1).is_empty());
    assert!(feed_candidates(&source, &maintenance, 1).is_empty());
}

#[test]
fn a_non_https_page_gives_alerts_without_a_link() {
    let signal = statuspage(
        json!([{"id": "c1", "name": "API", "status": "partial_outage"}]),
        json!([]),
    );

    let candidates = feed_candidates(
        &source("http://127.0.0.1:8765", FeedFormat::Statuspage),
        &signal,
        1,
    );

    assert_eq!(candidates[0].draft.link, None);
}

#[test]
fn connection_and_instance_alerts_have_no_link() {
    let connection =
        super::super::signals::connection("Stub", crate::domain::FailureKind::Network, 2).unwrap();
    let component = ComponentHealth {
        name: "kustomize-controller".to_owned(),
        level: HealthLevel::MajorOutage,
        detail: Some("scaled to zero".to_owned()),
    };
    let degraded = ComponentHealth {
        name: "helm-controller".to_owned(),
        level: HealthLevel::Degraded,
        detail: None,
    };

    let from_connection = connection_candidate(&connection, 3);
    let from_instance = instance_candidates("Stub Two", &[component, degraded], 3);

    assert_eq!(from_connection.draft.link, None);
    assert_eq!(from_connection.draft.title, "Can't reach Stub");
    assert_eq!(from_connection.provider_ids, [3]);
    assert_eq!(from_instance.len(), 1);
    assert_eq!(from_instance[0].key, "component:kustomize-controller");
    assert_eq!(
        from_instance[0].draft.title,
        "kustomize-controller on Stub Two: scaled to zero"
    );
    assert_eq!(from_instance[0].draft.link, None);
    assert_eq!(connection_recovery("Stub").link, None);
    assert_eq!(instance_recovery("Stub Two").link, None);
}

#[test]
fn merging_unions_the_providers_of_the_same_key() {
    let candidate = |key: &str, id: i64, level| Candidate {
        key: key.to_owned(),
        draft: Draft {
            level,
            ..draft(key)
        },
        provider_ids: vec![id],
    };

    let merged = merge(vec![
        candidate("a", 2, HealthLevel::PartialOutage),
        candidate("b", 3, HealthLevel::PartialOutage),
        candidate("a", 1, HealthLevel::MajorOutage),
        candidate("a", 2, HealthLevel::PartialOutage),
    ]);

    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].provider_ids, [1, 2]);
    assert_eq!(merged[0].draft.level, HealthLevel::MajorOutage);
    assert_eq!(merged[1].provider_ids, [3]);
}

#[test]
fn alerts_turn_into_events_with_the_filtered_provider_ids() {
    let mut tracker = AlertTracker::default();
    let now = Instant::now();
    tracker.observe(SUBJECT, now, healthy());
    let alert = tracker
        .observe(SUBJECT, now, outage(&["a"], &[1, 2]))
        .remove(0);

    let event = alert.into_event(vec![2]);

    let CoreEvent::ProviderHealthAlert {
        provider_ids,
        recovered,
        notification_key,
        ..
    } = event
    else {
        panic!("wrong event");
    };
    assert_eq!(provider_ids, [2]);
    assert!(!recovered);
    assert_eq!(notification_key, SUBJECT);
}
