pub mod statusio;
pub mod statuspage;
pub mod text;

use std::collections::{
    HashMap,
    HashSet,
};

use chrono::{
    DateTime,
    Duration,
    Utc,
};
use pipedash_plugin_api::{
    ComponentFilter,
    ComponentHealth,
    FeedFormat,
    HealthLevel,
};
use reqwest::Url;
use serde::{
    de::DeserializeOwned,
    Deserialize,
    Deserializer,
};
use thiserror::Error;

pub use self::statusio::parse_statusio;
pub use self::statuspage::parse_statuspage;
use crate::domain::health::{
    Incident,
    Maintenance,
};

pub const MAX_FEED_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_INCIDENTS: usize = 20;
pub const MAX_COMPONENTS: usize = 50;
pub const MAX_MAINTENANCES: usize = 10;

const MAINTENANCE_HORIZON_HOURS: i64 = 24;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FeedError {
    #[error("status feed is {0} bytes, over the 2 MiB limit")]
    TooLarge(usize),
    #[error("invalid status feed: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedComponent {
    pub id: String,
    pub name: String,
    pub level: HealthLevel,
    pub detail: Option<&'static str>,
    pub children: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedIncident {
    pub id: String,
    pub name: String,
    pub level: HealthLevel,
    pub phase: &'static str,
    pub latest_update: Option<String>,
    pub shortlink: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub components: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedMaintenance {
    pub id: String,
    pub name: String,
    pub in_progress: bool,
    pub shortlink: Option<String>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub components: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFeed {
    pub format: FeedFormat,
    pub components: Vec<FeedComponent>,
    pub incidents: Vec<FeedIncident>,
    pub maintenances: Vec<FeedMaintenance>,
    pub unknown: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedSignal {
    pub level: HealthLevel,
    pub components: Vec<ComponentHealth>,
    pub incidents: Vec<Incident>,
    pub maintenances: Vec<Maintenance>,
    pub incident_ids: Vec<String>,
    pub filter_fallback: bool,
    pub watched: Vec<ComponentHealth>,
    pub other_incidents: Vec<Incident>,
}

pub fn evaluate(
    feed: &ParsedFeed, filter: &ComponentFilter, page_url: &Url, now: DateTime<Utc>,
) -> FeedSignal {
    let (watched, filter_fallback) = watched_ids(feed, filter);
    let is_watched = |component: &FeedComponent| match &watched {
        Some(ids) => ids.contains(component.id.as_str()),
        None => component.children.is_empty(),
    };
    let applies = |ids: &[String]| match &watched {
        Some(watched) => ids.is_empty() || ids.iter().any(|id| watched.contains(id.as_str())),
        None => true,
    };
    let affects = |ids: &[String]| match &watched {
        Some(watched) => ids.iter().any(|id| watched.contains(id.as_str())),
        None => true,
    };
    let base = with_trailing_slash(page_url);
    let link = |id: &str, shortlink: Option<&str>| {
        let candidate = match feed.format {
            FeedFormat::Statuspage => join_item(&base, id).or_else(|| shortlink.map(str::to_owned)),
            _ => None,
        };
        https_link(candidate.as_deref(), page_url)
    };

    let to_health = |c: &FeedComponent| ComponentHealth {
        name: c.name.clone(),
        level: c.level,
        detail: c.detail.map(str::to_owned),
    };
    let to_incident = |i: &FeedIncident, updated_at: DateTime<Utc>| Incident {
        id: i.id.clone(),
        name: i.name.clone(),
        level: i.level,
        phase: i.phase.to_owned(),
        latest_update: i.latest_update.clone(),
        url: link(&i.id, i.shortlink.as_deref()),
        started_at: i.started_at,
        updated_at,
    };
    let watched_components: Vec<ComponentHealth> = feed
        .components
        .iter()
        .filter(|c| is_watched(c) && (watched.is_some() || c.level > HealthLevel::Operational))
        .take(MAX_COMPONENTS)
        .map(to_health)
        .collect();

    let mut components: Vec<&FeedComponent> = feed
        .components
        .iter()
        .filter(|c| c.level > HealthLevel::Operational && is_watched(c))
        .collect();
    let mut level = components
        .iter()
        .map(|c| c.level)
        .max()
        .unwrap_or(HealthLevel::Operational);
    components.sort_by_key(|c| std::cmp::Reverse(c.level));
    components.truncate(MAX_COMPONENTS);

    let mut incidents: Vec<(&FeedIncident, DateTime<Utc>)> = feed
        .incidents
        .iter()
        .filter(|i| affects(&i.components))
        .map(|i| (i, i.updated_at.unwrap_or(now)))
        .collect();
    level = level.max(
        incidents
            .iter()
            .map(|(i, _)| i.level)
            .max()
            .unwrap_or(HealthLevel::Operational),
    );
    incidents.sort_by(|(a, a_at), (b, b_at)| {
        b.level
            .cmp(&a.level)
            .then_with(|| b_at.cmp(a_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut incident_ids: Vec<String> = incidents.iter().map(|(i, _)| i.id.clone()).collect();
    incident_ids.sort();
    incident_ids.dedup();
    incidents.truncate(MAX_INCIDENTS);
    let mut other_incidents: Vec<Incident> = feed
        .incidents
        .iter()
        .filter(|i| !affects(&i.components))
        .map(|i| to_incident(i, i.updated_at.unwrap_or(now)))
        .collect();
    other_incidents.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    other_incidents.truncate(MAX_INCIDENTS);

    let horizon = now + Duration::hours(MAINTENANCE_HORIZON_HOURS);
    let mut maintenances: Vec<&FeedMaintenance> = feed
        .maintenances
        .iter()
        .filter(|m| {
            applies(&m.components)
                && (m.in_progress
                    || (m.starts_at.is_some_and(|start| start <= horizon)
                        && m.ends_at.is_none_or(|end| end >= now)))
        })
        .collect();
    if maintenances.iter().any(|m| m.in_progress) {
        level = level.max(HealthLevel::Maintenance);
    }
    maintenances.sort_by(|a, b| {
        b.in_progress
            .cmp(&a.in_progress)
            .then_with(|| a.starts_at.cmp(&b.starts_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    maintenances.truncate(MAX_MAINTENANCES);

    FeedSignal {
        level,
        components: components.into_iter().map(to_health).collect(),
        incidents: incidents
            .into_iter()
            .map(|(i, updated_at)| to_incident(i, updated_at))
            .collect(),
        maintenances: maintenances
            .into_iter()
            .map(|m| Maintenance {
                id: m.id.clone(),
                name: m.name.clone(),
                in_progress: m.in_progress,
                starts_at: m.starts_at,
                ends_at: m.ends_at,
                url: link(&m.id, m.shortlink.as_deref()),
            })
            .collect(),
        incident_ids,
        filter_fallback,
        watched: watched_components,
        other_incidents,
    }
}

fn watched_ids<'a>(
    feed: &'a ParsedFeed, filter: &ComponentFilter,
) -> (Option<HashSet<&'a str>>, bool) {
    let ComponentFilter::Ids(ids) = filter else {
        return (None, false);
    };
    let by_id: HashMap<&str, &FeedComponent> =
        feed.components.iter().map(|c| (c.id.as_str(), c)).collect();
    let found: Vec<&FeedComponent> = ids
        .iter()
        .filter_map(|id| by_id.get(id.as_str()).copied())
        .collect();
    if found.is_empty() {
        return (None, true);
    }

    let mut watched = HashSet::new();
    for component in found {
        if component.children.is_empty() {
            watched.insert(component.id.as_str());
        } else {
            watched.extend(component.children.iter().map(String::as_str));
        }
    }
    (Some(watched), false)
}

fn with_trailing_slash(url: &Url) -> Url {
    let mut base = url.clone();
    if !base.path().ends_with('/') {
        let path = format!("{}/", base.path());
        base.set_path(&path);
    }
    base
}

fn join_item(base: &Url, id: &str) -> Option<String> {
    let plain = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if !plain {
        return None;
    }
    base.join(&format!("incidents/{id}")).ok().map(String::from)
}

fn https_link(candidate: Option<&str>, page_url: &Url) -> Option<String> {
    candidate
        .and_then(|link| Url::parse(link).ok())
        .filter(|link| link.scheme() == "https")
        .or_else(|| (page_url.scheme() == "https").then(|| page_url.clone()))
        .map(String::from)
}

pub(super) fn decode<T: DeserializeOwned>(body: &[u8]) -> Result<T, FeedError> {
    if body.len() > MAX_FEED_BYTES {
        return Err(FeedError::TooLarge(body.len()));
    }
    serde_json::from_slice(body).map_err(|e| FeedError::Invalid(e.to_string()))
}

pub(super) fn lenient_date<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<DateTime<Utc>>, D::Error> {
    Ok(Option::<String>::deserialize(deserializer)?
        .and_then(|text| DateTime::parse_from_rfc3339(&text).ok())
        .map(|date| date.with_timezone(&Utc)))
}

pub(super) fn note_unknown(unknown: &mut Vec<&'static str>, field: &'static str) {
    if !unknown.contains(&field) {
        unknown.push(field);
    }
}

#[cfg(test)]
mod tests {
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

    const GITHUB_WATCHED: [&str; 4] = [
        "br0l2tvcx85d",
        "brv1bkgrwx7q",
        "8l4ygp009s5s",
        "4230lsnqdsld",
    ];

    #[test]
    fn github_incident_on_operational_components_makes_the_feed_degraded() {
        let feed = parse_statuspage(fixture!("github.json")).unwrap();

        let signal = evaluate(
            &feed,
            &ids(&GITHUB_WATCHED),
            &url("https://www.githubstatus.com"),
            now(),
        );

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
            Some("https://www.githubstatus.com/incidents/wq8305lppdc2")
        );
        let update = incident.latest_update.as_deref().unwrap();
        assert!(update.starts_with("GitHub Projects update processing has improved"));
        assert!(update.contains("\nWe're continuing"));
        assert!(!update.contains('<'));
        assert!(feed.unknown.is_empty());
    }

    #[test]
    fn github_incident_on_unwatched_components_is_not_counted() {
        let feed = parse_statuspage(fixture!("github.json")).unwrap();

        let signal = evaluate(
            &feed,
            &ids(&["4230lsnqdsld"]),
            &url("https://www.githubstatus.com"),
            now(),
        );

        assert_eq!(signal.level, HealthLevel::Operational);
        assert!(signal.incidents.is_empty());
        assert!(signal.incident_ids.is_empty());
        assert!(!signal.filter_fallback);
    }

    #[test]
    fn bitbucket_fixture_is_operational() {
        let feed = parse_statuspage(fixture!("bitbucket.json")).unwrap();

        let signal = evaluate(
            &feed,
            &ids(&[
                "k0x2yw1435v7",
                "rzh2n85q7vtt",
                "rfzky0v13fbp",
                "c1qmcrcbc5zy",
            ]),
            &url("https://bitbucket.status.atlassian.com"),
            now(),
        );

        assert_eq!(signal.level, HealthLevel::Operational);
        assert!(!signal.filter_fallback);
        assert!(signal.components.is_empty() && signal.incidents.is_empty());
    }

    #[test]
    fn buildkite_group_expands_to_children_and_ids_beat_duplicate_names() {
        let body = mutate(fixture!("buildkite.json"), |v| {
            set_status(v, "sd4q7mk8n853", "partial_outage");
            set_status(v, "msyttmm8jpns", "major_outage");
        });
        let feed = parse_statuspage(&body).unwrap();
        let buildkite = url("https://www.buildkitestatus.com");

        let hosted = evaluate(&feed, &ids(&["56ph4s45xl9s"]), &buildkite, now());
        let rest_api = evaluate(&feed, &ids(&["r21z46hqn47c"]), &buildkite, now());
        let other_rest_api = evaluate(&feed, &ids(&["msyttmm8jpns"]), &buildkite, now());
        let everything = evaluate(&feed, &ComponentFilter::All, &buildkite, now());

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
    fn buildkite_fixture_is_operational_with_all_watched_ids() {
        let feed = parse_statuspage(fixture!("buildkite.json")).unwrap();

        let signal = evaluate(
            &feed,
            &ids(&[
                "r21z46hqn47c",
                "1mpkbbd1kh0h",
                "3n7fzwsysx86",
                "56ph4s45xl9s",
            ]),
            &url("https://www.buildkitestatus.com"),
            now(),
        );

        assert_eq!(signal.level, HealthLevel::Operational);
        assert!(!signal.filter_fallback);
    }

    #[test]
    fn gitlab_fixture_is_operational_and_container_outage_propagates() {
        let watched = ids(&[
            "5b36e05f02d06804c0834a09",
            "5d2f74932676bc45e4927ead",
            "67ea346990c050053ab18e89",
            "5b36e06c633e9004b3d624ad",
            "5e29c1403f4deb04c0d7f035",
            "650c87fbadb04b052efa2bc9",
        ]);
        let gitlab = url("https://status.gitlab.com");
        let feed = parse_statusio(fixture!("gitlab.json")).unwrap();
        let broken = mutate(fixture!("gitlab.json"), |v| {
            let status = v["result"]["status"].as_array_mut().unwrap();
            let macos = status
                .iter_mut()
                .find(|c| c["id"] == "650c87fbadb04b052efa2bc9")
                .unwrap();
            macos["containers"][1]["status_code"] = json!(500);
        });

        let operational = evaluate(&feed, &watched, &gitlab, now());
        let outage = evaluate(&parse_statusio(&broken).unwrap(), &watched, &gitlab, now());

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

        let signal = evaluate(
            &feed,
            &ComponentFilter::All,
            &url("https://status.gitlab.com"),
            now(),
        );

        assert_eq!(
            signal.incidents[0].url.as_deref(),
            Some("https://status.gitlab.com/")
        );
        assert_eq!(
            signal.maintenances[0].url.as_deref(),
            Some("https://status.gitlab.com/")
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
}
