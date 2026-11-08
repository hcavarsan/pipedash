use chrono::TimeZone;
use serde_json::{
    json,
    Value,
};

use super::*;

macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/status_feeds/",
            $name
        ))
    };
}

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 22, 23, 50, 0).unwrap()
}

fn url(text: &str) -> Url {
    Url::parse(text).unwrap()
}

fn page() -> Url {
    url("https://status.example.com")
}

fn ids(ids: &[&str]) -> ComponentFilter {
    ComponentFilter::Ids(ids.iter().map(|id| id.to_string()).collect())
}

fn mutate(bytes: &[u8], edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(bytes).unwrap();
    edit(&mut value);
    serde_json::to_vec(&value).unwrap()
}

fn set_status(value: &mut Value, id: &str, status: &str) {
    let component = value["components"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["id"] == id)
        .unwrap();
    component["status"] = json!(status);
}

fn sp(components: Value, incidents: Value, maintenances: Value) -> ParsedFeed {
    let body = json!({
        "components": components,
        "incidents": incidents,
        "scheduled_maintenances": maintenances,
    });
    parse_statuspage(body.to_string().as_bytes()).unwrap()
}

fn component(id: &str, status: &str) -> Value {
    json!({"id": id, "name": format!("Component {id}"), "status": status})
}

fn incident(id: &str, impact: &str, components: &[&str]) -> Value {
    json!({
        "id": id,
        "name": format!("Incident {id}"),
        "status": "investigating",
        "impact": impact,
        "updated_at": "2026-10-22T22:00:00.000Z",
        "components": components.iter().map(|c| json!({"id": c})).collect::<Vec<_>>(),
    })
}

fn maintenance(id: &str, status: &str, start_hours: i64, components: &[&str]) -> Value {
    let start = now() + Duration::hours(start_hours);
    json!({
        "id": id,
        "name": format!("Maintenance {id}"),
        "status": status,
        "scheduled_for": start.to_rfc3339(),
        "scheduled_until": (start + Duration::hours(2)).to_rfc3339(),
        "components": components.iter().map(|c| json!({"id": c})).collect::<Vec<_>>(),
    })
}

const PAGE_URL: &str = "https://status.example.test";

const WATCHED: [&str; 4] = [
    "br0l2tvcx85d",
    "brv1bkgrwx7q",
    "8l4ygp009s5s",
    "4230lsnqdsld",
];

#[test]
fn statuspage_incident_on_operational_components_makes_the_feed_degraded() {
    let feed = parse_statuspage(fixture!("statuspage.json")).unwrap();

    let signal = evaluate(&feed, &ids(&WATCHED), &url(PAGE_URL), now());

    assert!(feed
        .components
        .iter()
        .all(|c| c.level == HealthLevel::Operational));
    assert_eq!(signal.level, HealthLevel::Degraded);
    assert!(signal.components.is_empty());
    assert_eq!(signal.incident_ids, ["wq8305lppdc2"]);
    let incident = &signal.incidents[0];
    assert_eq!(incident.name, "Project updates delayed");
    assert_eq!(incident.level, HealthLevel::Degraded);
    assert_eq!(incident.phase, "Investigating");
    assert_eq!(
        incident.url.as_deref(),
        Some("https://status.example.test/incidents/wq8305lppdc2")
    );
    let update = incident.latest_update.as_deref().unwrap();
    assert!(update.starts_with("Example Projects update processing has improved"));
    assert!(update.contains("\nWe're continuing"));
    assert!(!update.contains('<'));
    assert!(feed.unknown.is_empty());
}

#[test]
fn statuspage_incident_on_unwatched_components_is_not_counted() {
    let feed = parse_statuspage(fixture!("statuspage.json")).unwrap();

    let signal = evaluate(&feed, &ids(&["4230lsnqdsld"]), &url(PAGE_URL), now());

    assert_eq!(signal.level, HealthLevel::Operational);
    assert!(signal.incidents.is_empty());
    assert!(signal.incident_ids.is_empty());
    assert!(!signal.filter_fallback);
}

#[test]
fn statuspage_without_incidents_or_maintenances_is_operational() {
    let body = mutate(fixture!("statuspage.json"), |v| {
        v["incidents"] = json!([]);
        v["scheduled_maintenances"] = json!([]);
    });
    let feed = parse_statuspage(&body).unwrap();

    let signal = evaluate(&feed, &ids(&WATCHED), &url(PAGE_URL), now());

    assert_eq!(signal.level, HealthLevel::Operational);
    assert!(!signal.filter_fallback);
    assert!(signal.components.is_empty() && signal.incidents.is_empty());
}

#[test]
fn statuspage_group_expands_to_children_and_ids_beat_duplicate_names() {
    let body = mutate(fixture!("statuspage_groups.json"), |v| {
        set_status(v, "sd4q7mk8n853", "partial_outage");
        set_status(v, "msyttmm8jpns", "major_outage");
    });
    let feed = parse_statuspage(&body).unwrap();
    let page = url(PAGE_URL);

    let hosted = evaluate(&feed, &ids(&["56ph4s45xl9s"]), &page, now());
    let rest_api = evaluate(&feed, &ids(&["r21z46hqn47c"]), &page, now());
    let other_rest_api = evaluate(&feed, &ids(&["msyttmm8jpns"]), &page, now());
    let everything = evaluate(&feed, &ComponentFilter::All, &page, now());

    assert_eq!(hosted.level, HealthLevel::PartialOutage);
    assert_eq!(hosted.components.len(), 1);
    assert_eq!(hosted.components[0].name, "Linux (ARM64)");
    assert_eq!(rest_api.level, HealthLevel::Operational);
    assert_eq!(other_rest_api.level, HealthLevel::MajorOutage);
    assert_eq!(other_rest_api.components[0].name, "REST API");
    assert_eq!(everything.level, HealthLevel::MajorOutage);
    let names: Vec<_> = everything
        .components
        .iter()
        .map(|c| (c.name.as_str(), c.level))
        .collect();
    assert_eq!(
        names,
        [
            ("REST API", HealthLevel::MajorOutage),
            ("Linux (ARM64)", HealthLevel::PartialOutage)
        ]
    );
}

#[test]
fn statuspage_groups_fixture_is_operational_with_all_watched_ids() {
    let feed = parse_statuspage(fixture!("statuspage_groups.json")).unwrap();

    let signal = evaluate(
        &feed,
        &ids(&[
            "r21z46hqn47c",
            "1mpkbbd1kh0h",
            "3n7fzwsysx86",
            "56ph4s45xl9s",
        ]),
        &url(PAGE_URL),
        now(),
    );

    assert_eq!(signal.level, HealthLevel::Operational);
    assert!(!signal.filter_fallback);
}

#[test]
fn statusio_fixture_is_operational_and_container_outage_propagates() {
    let watched = ids(&[
        "5b36e05f02d06804c0834a09",
        "5d2f74932676bc45e4927ead",
        "67ea346990c050053ab18e89",
        "5b36e06c633e9004b3d624ad",
        "5e29c1403f4deb04c0d7f035",
        "650c87fbadb04b052efa2bc9",
    ]);
    let page = url(PAGE_URL);
    let feed = parse_statusio(fixture!("statusio.json")).unwrap();
    let broken = mutate(fixture!("statusio.json"), |v| {
        let status = v["result"]["status"].as_array_mut().unwrap();
        let macos = status
            .iter_mut()
            .find(|c| c["id"] == "650c87fbadb04b052efa2bc9")
            .unwrap();
        macos["containers"][1]["status_code"] = json!(500);
    });

    let operational = evaluate(&feed, &watched, &page, now());
    let outage = evaluate(&parse_statusio(&broken).unwrap(), &watched, &page, now());

    assert_eq!(operational.level, HealthLevel::Operational);
    assert!(!operational.filter_fallback);
    assert!(operational.components.is_empty() && operational.incidents.is_empty());
    assert_eq!(outage.level, HealthLevel::MajorOutage);
    assert_eq!(outage.components[0].name, "CI/CD - Hosted runners on macOS");
}

#[test]
fn component_outages_use_the_worst_watched_level_worst_first() {
    let feed = sp(
        json!([
            component("a", "degraded_performance"),
            component("b", "major_outage"),
            component("c", "partial_outage"),
            component("d", "major_outage"),
        ]),
        json!([]),
        json!([]),
    );

    let signal = evaluate(&feed, &ids(&["a", "b", "c"]), &page(), now());

    assert_eq!(signal.level, HealthLevel::MajorOutage);
    let levels: Vec<_> = signal.components.iter().map(|c| c.level).collect();
    assert_eq!(
        levels,
        [
            HealthLevel::MajorOutage,
            HealthLevel::PartialOutage,
            HealthLevel::Degraded
        ]
    );
    assert_eq!(
        evaluate(&feed, &ids(&["a"]), &page(), now()).level,
        HealthLevel::Degraded
    );
}

#[test]
fn incident_impact_raises_the_level_and_none_is_only_listed() {
    let components = json!([component("a", "operational")]);
    let level_of = |impact: &str| {
        let feed = sp(
            components.clone(),
            json!([incident("i", impact, &["a"])]),
            json!([]),
        );
        let signal = evaluate(&feed, &ids(&["a"]), &page(), now());
        assert_eq!(signal.incidents.len(), 1);
        signal.level
    };

    assert_eq!(level_of("none"), HealthLevel::Operational);
    assert_eq!(level_of("minor"), HealthLevel::Degraded);
    assert_eq!(level_of("major"), HealthLevel::PartialOutage);
    assert_eq!(level_of("critical"), HealthLevel::MajorOutage);
    assert_eq!(level_of("maintenance"), HealthLevel::Maintenance);
}

#[test]
fn resolved_postmortem_and_monitoring_incidents() {
    let mut resolved = incident("done", "critical", &["a"]);
    resolved["status"] = json!("resolved");
    let mut postmortem = incident("post", "critical", &["a"]);
    postmortem["status"] = json!("postmortem");
    let mut monitoring = incident("watch", "major", &["a"]);
    monitoring["status"] = json!("monitoring");
    let feed = sp(
        json!([component("a", "operational")]),
        json!([resolved, postmortem, monitoring]),
        json!([]),
    );

    let signal = evaluate(&feed, &ids(&["a"]), &page(), now());

    assert_eq!(signal.incident_ids, ["watch"]);
    assert_eq!(signal.level, HealthLevel::PartialOutage);
    assert_eq!(signal.incidents[0].phase, "Monitoring a fix");
}

#[test]
fn untagged_incidents_count_only_when_the_whole_page_is_watched() {
    let feed = sp(
        json!([
            component("a", "operational"),
            component("b", "partial_outage")
        ]),
        json!([
            incident("wide", "major", &[]),
            incident("mine", "minor", &["a"]),
            incident("theirs", "critical", &["b"]),
        ]),
        json!([]),
    );

    let signal = evaluate(&feed, &ids(&["a"]), &page(), now());
    let all = evaluate(&feed, &ComponentFilter::All, &page(), now());

    assert_eq!(signal.incident_ids, ["mine"]);
    assert_eq!(signal.level, HealthLevel::Degraded);
    let others: Vec<_> = signal
        .other_incidents
        .iter()
        .map(|i| i.id.as_str())
        .collect();
    assert_eq!(others.len(), 2);
    assert!(others.contains(&"wide") && others.contains(&"theirs"));
    let watched: Vec<_> = signal
        .watched
        .iter()
        .map(|c| (c.name.as_str(), c.level))
        .collect();
    assert_eq!(watched, [("Component a", HealthLevel::Operational)]);
    assert_eq!(all.incident_ids, ["mine", "theirs", "wide"]);
    assert_eq!(all.level, HealthLevel::MajorOutage);
    assert!(all.other_incidents.is_empty());
    let all_watched: Vec<_> = all.watched.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(all_watched, ["Component b"]);
}

#[test]
fn incidents_on_a_watched_group_child_count_for_the_group_filter() {
    let feed = sp(
        json!([
            {"id": "g", "name": "Group", "status": "operational", "group": true, "components": ["a", "b"]},
            component("a", "operational"),
            component("b", "operational"),
            component("c", "operational"),
        ]),
        json!([
            incident("child", "minor", &["b"]),
            incident("other", "major", &["c"])
        ]),
        json!([]),
    );

    let signal = evaluate(&feed, &ids(&["g"]), &page(), now());

    assert_eq!(signal.incident_ids, ["child"]);
}

#[test]
fn filter_with_no_known_ids_falls_back_to_all_with_a_flag() {
    let feed = sp(
        json!([
            component("a", "major_outage"),
            component("b", "operational")
        ]),
        json!([]),
        json!([]),
    );

    let fallback = evaluate(&feed, &ids(&["ghost", "phantom"]), &page(), now());
    let partial = evaluate(&feed, &ids(&["b", "ghost"]), &page(), now());
    let empty = evaluate(&feed, &ids(&[]), &page(), now());

    assert!(fallback.filter_fallback);
    assert_eq!(fallback.level, HealthLevel::MajorOutage);
    assert!(!partial.filter_fallback);
    assert_eq!(partial.level, HealthLevel::Operational);
    assert!(empty.filter_fallback);
}

#[test]
fn maintenance_in_progress_raises_to_maintenance_level() {
    let feed = sp(
        json!([component("a", "operational")]),
        json!([]),
        json!([maintenance("m", "in_progress", -1, &["a"])]),
    );

    let signal = evaluate(&feed, &ids(&["a"]), &page(), now());

    assert_eq!(signal.level, HealthLevel::Maintenance);
    assert!(signal.maintenances[0].in_progress);
}

#[test]
fn maintenance_within_24_hours_is_listed_without_raising_the_level() {
    let feed = sp(
        json!([component("a", "operational")]),
        json!([]),
        json!([
            maintenance("soon", "scheduled", 23, &["a"]),
            maintenance("later", "scheduled", 25, &["a"]),
            maintenance("elsewhere", "scheduled", 2, &["zzz"]),
            maintenance("page", "scheduled", 3, &[]),
        ]),
    );

    let signal = evaluate(&feed, &ids(&["a"]), &page(), now());

    assert_eq!(signal.level, HealthLevel::Operational);
    let listed: Vec<_> = signal
        .maintenances
        .iter()
        .map(|m| (m.id.as_str(), m.in_progress))
        .collect();
    assert_eq!(listed, [("page", false), ("soon", false)]);
}

#[test]
fn scheduled_maintenance_that_already_ended_is_dropped() {
    let feed = sp(
        json!([component("a", "operational")]),
        json!([]),
        json!([maintenance("stale", "scheduled", -5, &["a"])]),
    );

    assert!(evaluate(&feed, &ids(&["a"]), &page(), now())
        .maintenances
        .is_empty());
}

#[test]
fn status_io_maintenance_active_and_upcoming() {
    let at = |hours: i64| (now() + Duration::hours(hours)).to_rfc3339();
    let body = json!({"result": {
        "status": [{"id": "a", "name": "A", "status_code": 100, "containers": []}],
        "maintenance": {
            "active": [{"_id": "now", "name": "Now", "datetime_planned_start": at(-1), "datetime_planned_end": at(1)}],
            "upcoming": [
                {"_id": "soon", "name": "Soon", "datetime_planned_start": at(5), "datetime_planned_end": at(6)},
                {"_id": "far", "name": "Far", "datetime_planned_start": at(72), "datetime_planned_end": at(73)},
            ],
        },
    }});
    let feed = parse_statusio(body.to_string().as_bytes()).unwrap();

    let signal = evaluate(
        &feed,
        &ids(&["a"]),
        &url("https://status.example.com"),
        now(),
    );

    assert_eq!(signal.level, HealthLevel::Maintenance);
    let listed: Vec<_> = signal
        .maintenances
        .iter()
        .map(|m| (m.id.as_str(), m.in_progress))
        .collect();
    assert_eq!(listed, [("now", true), ("soon", false)]);
}

#[test]
fn unknown_enum_strings_are_reported_not_fatal() {
    let feed = sp(
        json!([component("a", "levitating")]),
        json!([incident("i", "galactic", &["a"])]),
        json!([]),
    );

    let signal = evaluate(&feed, &ids(&["a"]), &page(), now());

    assert_eq!(feed.unknown, ["component status", "incident impact"]);
    assert_eq!(signal.level, HealthLevel::Operational);
}

#[test]
fn statuspage_links_point_to_the_incidents_path_of_the_page() {
    let feed = sp(
        json!([component("a", "operational")]),
        json!([incident("inc1", "minor", &["a"])]),
        json!([maintenance("mnt1", "scheduled", 2, &["a"])]),
    );

    let plain = evaluate(
        &feed,
        &ids(&["a"]),
        &url("https://status.example.com"),
        now(),
    );
    let nested = evaluate(
        &feed,
        &ids(&["a"]),
        &url("https://example.com/status"),
        now(),
    );

    assert_eq!(
        plain.incidents[0].url.as_deref(),
        Some("https://status.example.com/incidents/inc1")
    );
    assert_eq!(
        plain.maintenances[0].url.as_deref(),
        Some("https://status.example.com/incidents/mnt1")
    );
    assert_eq!(
        nested.incidents[0].url.as_deref(),
        Some("https://example.com/status/incidents/inc1")
    );
}

#[test]
fn status_io_items_link_to_the_page_url() {
    let body = json!({"result": {
        "status": [{"id": "a", "name": "A", "status_code": 100, "containers": []}],
        "incidents": [{
            "_id": "inc", "name": "Down",
            "messages": [{"state": 100, "status": 500, "datetime": "2026-10-22T10:00:00.000Z", "details": "x"}],
        }],
        "maintenance": {"active": [{"_id": "mnt", "name": "M"}]},
    }});
    let feed = parse_statusio(body.to_string().as_bytes()).unwrap();

    let signal = evaluate(&feed, &ComponentFilter::All, &url(PAGE_URL), now());

    assert_eq!(
        signal.incidents[0].url.as_deref(),
        Some("https://status.example.test/")
    );
    assert_eq!(
        signal.maintenances[0].url.as_deref(),
        Some("https://status.example.test/")
    );
}

#[test]
fn non_https_page_url_gives_no_link() {
    let feed = sp(
        json!([component("a", "operational")]),
        json!([incident("inc1", "minor", &["a"])]),
        json!([maintenance("mnt1", "in_progress", -1, &["a"])]),
    );

    let signal = evaluate(&feed, &ids(&["a"]), &url("http://127.0.0.1:8080"), now());

    assert_eq!(signal.incidents[0].url, None);
    assert_eq!(signal.maintenances[0].url, None);
}

#[test]
fn shortlink_is_used_only_when_the_join_fails_and_only_if_https() {
    let page = url("https://status.example.com");

    assert_eq!(
        https_link(
            join_item(&page, "bad id/../x")
                .or(Some("https://stspg.io/abc".to_string()))
                .as_deref(),
            &page
        )
        .as_deref(),
        Some("https://stspg.io/abc")
    );
    assert_eq!(
        https_link(Some("http://stspg.io/abc"), &page).as_deref(),
        Some("https://status.example.com/")
    );
    assert_eq!(
        https_link(Some("http://stspg.io/abc"), &url("http://127.0.0.1")),
        None
    );
    assert_eq!(
        https_link(Some("not a url"), &url("http://127.0.0.1")),
        None
    );
}

#[test]
fn shortlink_in_a_feed_replaces_an_unjoinable_id() {
    let mut odd = incident("a/b", "minor", &[]);
    odd["shortlink"] = json!("http://stspg.io/abc");
    let mut good = incident("c d", "minor", &[]);
    good["shortlink"] = json!("https://stspg.io/xyz");
    let feed = sp(
        json!([component("a", "operational")]),
        json!([odd, good]),
        json!([]),
    );

    let signal = evaluate(&feed, &ComponentFilter::All, &page(), now());

    let urls: Vec<_> = signal
        .incidents
        .iter()
        .map(|i| (i.id.as_str(), i.url.as_deref()))
        .collect();
    assert_eq!(
        urls,
        [
            ("a/b", Some("https://status.example.com/")),
            ("c d", Some("https://stspg.io/xyz"))
        ]
    );
}

#[test]
fn caps_limit_components_incidents_and_maintenances_but_not_alert_ids() {
    let components: Vec<_> = (0..60)
        .map(|n| component(&format!("c{n:02}"), "partial_outage"))
        .collect();
    let incidents: Vec<_> = (0..25)
        .map(|n| incident(&format!("i{n:02}"), "minor", &[]))
        .collect();
    let maintenances: Vec<_> = (0..15)
        .map(|n| maintenance(&format!("m{n:02}"), "in_progress", -1, &[]))
        .collect();
    let feed = sp(json!(components), json!(incidents), json!(maintenances));

    let signal = evaluate(&feed, &ComponentFilter::All, &page(), now());

    assert_eq!(signal.components.len(), MAX_COMPONENTS);
    assert_eq!(signal.incidents.len(), MAX_INCIDENTS);
    assert_eq!(signal.maintenances.len(), MAX_MAINTENANCES);
    assert_eq!(signal.incident_ids.len(), 25);
    assert_eq!(signal.level, HealthLevel::PartialOutage);
}

#[test]
fn incidents_sort_by_level_then_recency() {
    let mut older = incident("older", "major", &[]);
    older["updated_at"] = json!("2026-10-22T08:00:00.000Z");
    let feed = sp(
        json!([component("a", "operational")]),
        json!([
            incident("minor", "minor", &[]),
            older,
            incident("newer", "major", &[])
        ]),
        json!([]),
    );

    let signal = evaluate(&feed, &ComponentFilter::All, &page(), now());

    let order: Vec<_> = signal.incidents.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(order, ["newer", "older", "minor"]);
}

#[test]
fn bodies_over_the_limit_are_rejected() {
    let body = vec![b' '; MAX_FEED_BYTES + 1];

    assert_eq!(
        parse_statuspage(&body),
        Err(FeedError::TooLarge(MAX_FEED_BYTES + 1))
    );
    assert_eq!(
        parse_statusio(&body),
        Err(FeedError::TooLarge(MAX_FEED_BYTES + 1))
    );
}

#[test]
fn a_body_at_the_limit_is_still_parsed() {
    let mut body = br#"{"components":[]}"#.to_vec();
    body.resize(MAX_FEED_BYTES, b' ');

    assert!(parse_statuspage(&body).is_ok());
}
