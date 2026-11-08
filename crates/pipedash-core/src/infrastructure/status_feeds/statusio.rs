use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    FeedFormat,
    HealthLevel,
};
use serde::{
    Deserialize,
    Deserializer,
};
use serde_json::Value;

use super::text::{
    to_plain,
    NAME_MAX_CHARS,
    UPDATE_MAX_CHARS,
};
use super::{
    decode,
    lenient_date,
    note_unknown,
    FeedComponent,
    FeedError,
    FeedIncident,
    FeedMaintenance,
    ParsedFeed,
};

const SECURITY_EVENT: &str = "Security event";
const STATE_RESOLVED: u32 = 400;

#[derive(Deserialize)]
struct Envelope {
    result: Feed,
}

#[derive(Deserialize)]
struct Feed {
    status: Vec<RawComponent>,
    #[serde(default)]
    incidents: Vec<RawIncident>,
    #[serde(default)]
    maintenance: RawMaintenances,
}

#[derive(Deserialize)]
struct RawComponent {
    id: String,
    name: String,
    #[serde(default, deserialize_with = "code")]
    status_code: Option<u32>,
    #[serde(default)]
    containers: Vec<RawContainer>,
}

#[derive(Deserialize)]
struct RawContainer {
    #[serde(default, deserialize_with = "code")]
    status_code: Option<u32>,
}

#[derive(Deserialize)]
struct RawIncident {
    #[serde(rename = "_id", alias = "id")]
    id: String,
    name: String,
    #[serde(default, deserialize_with = "lenient_date")]
    datetime_open: Option<DateTime<Utc>>,
    #[serde(default)]
    messages: Vec<RawMessage>,
    #[serde(default)]
    components_affected: Vec<Affected>,
}

#[derive(Deserialize)]
struct RawMessage {
    #[serde(default)]
    details: Option<String>,
    #[serde(default, deserialize_with = "code")]
    state: Option<u32>,
    #[serde(default, deserialize_with = "code")]
    status: Option<u32>,
    #[serde(default, deserialize_with = "lenient_date")]
    datetime: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
struct Affected {
    #[serde(rename = "_id", alias = "id")]
    id: String,
}

#[derive(Deserialize, Default)]
struct RawMaintenances {
    #[serde(default)]
    active: Vec<RawMaintenance>,
    #[serde(default)]
    upcoming: Vec<RawMaintenance>,
}

#[derive(Deserialize)]
struct RawMaintenance {
    #[serde(rename = "_id", alias = "id")]
    id: String,
    name: String,
    #[serde(default, deserialize_with = "lenient_date")]
    datetime_planned_start: Option<DateTime<Utc>>,
    #[serde(default, deserialize_with = "lenient_date")]
    datetime_planned_end: Option<DateTime<Utc>>,
    #[serde(default)]
    components_affected: Vec<Affected>,
}

fn code<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u32>, D::Error> {
    Ok(match Option::<Value>::deserialize(deserializer)? {
        Some(Value::Number(n)) => n.as_u64().and_then(|n| u32::try_from(n).ok()),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    })
}

pub fn parse_statusio(body: &[u8]) -> Result<ParsedFeed, FeedError> {
    let Envelope { result } = decode(body)?;
    let mut unknown = Vec::new();

    let components = result
        .status
        .into_iter()
        .map(|c| {
            let (level, detail) = component_level(&c, &mut unknown);
            FeedComponent {
                id: c.id,
                name: to_plain(&c.name, NAME_MAX_CHARS),
                level,
                detail,
                children: Vec::new(),
            }
        })
        .collect();

    let incidents = result
        .incidents
        .into_iter()
        .filter_map(|raw| incident(raw, &mut unknown))
        .collect();

    let maintenances = result
        .maintenance
        .active
        .into_iter()
        .map(|raw| maintenance(raw, true))
        .chain(
            result
                .maintenance
                .upcoming
                .into_iter()
                .map(|raw| maintenance(raw, false)),
        )
        .collect();

    Ok(ParsedFeed {
        format: FeedFormat::StatusIo,
        components,
        incidents,
        maintenances,
        unknown,
    })
}

fn code_level(code: u32) -> Option<(HealthLevel, Option<&'static str>)> {
    match code {
        100 => Some((HealthLevel::Operational, None)),
        200 => Some((HealthLevel::Maintenance, None)),
        300 => Some((HealthLevel::Degraded, None)),
        400 => Some((HealthLevel::PartialOutage, None)),
        500 => Some((HealthLevel::MajorOutage, None)),
        600 => Some((HealthLevel::Degraded, Some(SECURITY_EVENT))),
        _ => None,
    }
}

fn known_level(
    code: Option<u32>, field: &'static str, unknown: &mut Vec<&'static str>,
) -> Option<(HealthLevel, Option<&'static str>)> {
    let level = code.and_then(code_level);
    if level.is_none() {
        note_unknown(unknown, field);
    }
    level
}

fn component_level(
    component: &RawComponent, unknown: &mut Vec<&'static str>,
) -> (HealthLevel, Option<&'static str>) {
    std::iter::once(component.status_code)
        .chain(component.containers.iter().map(|c| c.status_code))
        .filter_map(|code| known_level(code, "component status_code", unknown))
        .max_by_key(|(level, detail)| (*level, detail.is_some()))
        .unwrap_or((HealthLevel::Operational, None))
}

fn incident(raw: RawIncident, unknown: &mut Vec<&'static str>) -> Option<FeedIncident> {
    let newest = raw.messages.iter().max_by_key(|m| m.datetime);
    let state = newest.and_then(|m| m.state);
    let phase = match state {
        Some(STATE_RESOLVED) => return None,
        Some(100) => "Investigating",
        Some(200) => "Identified",
        Some(300) => "Monitoring a fix",
        Some(_) => {
            note_unknown(unknown, "incident state");
            "Ongoing"
        }
        None => "Ongoing",
    };
    let level = match newest {
        Some(message) => known_level(message.status, "incident status", unknown)
            .map_or(HealthLevel::Operational, |(level, _)| level),
        None => HealthLevel::Operational,
    };

    Some(FeedIncident {
        level,
        phase,
        latest_update: newest
            .and_then(|m| m.details.as_deref())
            .map(|details| to_plain(details, UPDATE_MAX_CHARS))
            .filter(|text| !text.is_empty()),
        updated_at: newest.and_then(|m| m.datetime),
        id: raw.id,
        name: to_plain(&raw.name, NAME_MAX_CHARS),
        shortlink: None,
        started_at: raw.datetime_open,
        components: raw.components_affected.into_iter().map(|c| c.id).collect(),
    })
}

fn maintenance(raw: RawMaintenance, in_progress: bool) -> FeedMaintenance {
    FeedMaintenance {
        id: raw.id,
        name: to_plain(&raw.name, NAME_MAX_CHARS),
        in_progress,
        shortlink: None,
        starts_at: raw.datetime_planned_start,
        ends_at: raw.datetime_planned_end,
        components: raw.components_affected.into_iter().map(|c| c.id).collect(),
    }
}

#[cfg(test)]
mod tests;
