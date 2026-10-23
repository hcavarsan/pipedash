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
mod tests {
    use serde_json::json;

    use super::*;

    fn feed(result: serde_json::Value) -> ParsedFeed {
        parse_statusio(json!({ "result": result }).to_string().as_bytes()).unwrap()
    }

    fn component(id: &str, code: u32, containers: &[u32]) -> serde_json::Value {
        json!({
            "id": id,
            "name": id,
            "status_code": code,
            "containers": containers.iter().map(|c| json!({"id": "c", "status_code": c})).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn status_codes_map_to_levels() {
        let parsed = feed(json!({"status": [
            component("a", 100, &[]),
            component("b", 200, &[]),
            component("c", 300, &[]),
            component("d", 400, &[]),
            component("e", 500, &[]),
            component("f", 600, &[]),
        ]}));

        let seen: Vec<_> = parsed
            .components
            .iter()
            .map(|c| (c.level, c.detail))
            .collect();

        assert_eq!(
            seen,
            [
                (HealthLevel::Operational, None),
                (HealthLevel::Maintenance, None),
                (HealthLevel::Degraded, None),
                (HealthLevel::PartialOutage, None),
                (HealthLevel::MajorOutage, None),
                (HealthLevel::Degraded, Some("Security event")),
            ]
        );
    }

    #[test]
    fn component_level_is_the_worst_of_itself_and_its_containers() {
        let parsed = feed(json!({"status": [
            component("a", 100, &[100, 400, 300]),
            component("b", 500, &[100]),
            component("c", 100, &[100]),
        ]}));

        let levels: Vec<_> = parsed.components.iter().map(|c| c.level).collect();

        assert_eq!(
            levels,
            [
                HealthLevel::PartialOutage,
                HealthLevel::MajorOutage,
                HealthLevel::Operational
            ]
        );
    }

    #[test]
    fn incidents_use_the_newest_message_and_skip_resolved_ones() {
        let message = |state: u32, status: u32, at: &str, details: &str| json!({"state": state, "status": status, "datetime": at, "details": details});
        let parsed = feed(json!({
            "status": [],
            "incidents": [
                {
                    "_id": "open",
                    "name": "Runners &amp; queues",
                    "datetime_open": "2026-10-22T10:00:00.000Z",
                    "messages": [
                        message(300, 500, "2026-10-22T12:00:00.000Z", "Watching<br>closely"),
                        message(100, 300, "2026-10-22T10:00:00.000Z", "first"),
                    ],
                    "components_affected": [{"_id": "x", "name": "X"}],
                },
                {
                    "_id": "done",
                    "name": "Done",
                    "messages": [
                        message(100, 500, "2026-10-22T10:00:00.000Z", "first"),
                        message(400, 100, "2026-10-22T12:00:00.000Z", "fixed"),
                    ],
                },
                {
                    "_id": "bare",
                    "name": "No messages",
                },
            ],
        }));

        assert_eq!(parsed.incidents.len(), 2);
        let open = &parsed.incidents[0];
        assert_eq!(open.id, "open");
        assert_eq!(open.name, "Runners & queues");
        assert_eq!(open.level, HealthLevel::MajorOutage);
        assert_eq!(open.phase, "Monitoring a fix");
        assert_eq!(open.latest_update.as_deref(), Some("Watching\nclosely"));
        assert_eq!(open.components, ["x"]);
        assert_eq!(parsed.incidents[1].id, "bare");
        assert_eq!(parsed.incidents[1].level, HealthLevel::Operational);
    }

    #[test]
    fn maintenance_lists_follow_active_and_upcoming() {
        let parsed = feed(json!({
            "status": [],
            "maintenance": {
                "active": [{"_id": "a", "name": "A", "datetime_planned_start": "2026-10-22T10:00:00.000Z"}],
                "upcoming": [{"_id": "u", "name": "U", "datetime_planned_end": "2026-10-23T10:00:00.000Z"}],
            },
        }));

        let seen: Vec<_> = parsed
            .maintenances
            .iter()
            .map(|m| (m.id.as_str(), m.in_progress))
            .collect();

        assert_eq!(seen, [("a", true), ("u", false)]);
    }

    #[test]
    fn string_codes_are_accepted_and_unknown_codes_recorded_once() {
        let parsed = feed(json!({"status": [
            {"id": "a", "name": "A", "status_code": "300"},
            component("b", 900, &[901]),
        ]}));

        assert_eq!(parsed.components[0].level, HealthLevel::Degraded);
        assert_eq!(parsed.components[1].level, HealthLevel::Operational);
        assert_eq!(parsed.unknown, ["component status_code"]);
    }

    #[test]
    fn payload_without_result_is_rejected() {
        assert!(matches!(parse_statusio(b"{}"), Err(FeedError::Invalid(_))));
    }
}
