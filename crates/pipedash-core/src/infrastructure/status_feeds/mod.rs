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
mod tests;
