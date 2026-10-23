use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    FeedFormat,
    HealthLevel,
};
use serde::Deserialize;

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

#[derive(Deserialize)]
struct Summary {
    components: Vec<RawComponent>,
    #[serde(default)]
    incidents: Vec<RawIncident>,
    #[serde(default)]
    scheduled_maintenances: Vec<RawIncident>,
}

#[derive(Deserialize)]
struct RawComponent {
    id: String,
    name: String,
    #[serde(default)]
    status: Option<ComponentStatus>,
    #[serde(default)]
    components: Vec<String>,
}

#[derive(Deserialize)]
struct RawIncident {
    id: String,
    name: String,
    #[serde(default)]
    status: Option<Status>,
    #[serde(default)]
    impact: Option<Impact>,
    #[serde(default)]
    shortlink: Option<String>,
    #[serde(default, deserialize_with = "lenient_date")]
    started_at: Option<DateTime<Utc>>,
    #[serde(default, deserialize_with = "lenient_date")]
    updated_at: Option<DateTime<Utc>>,
    #[serde(default, deserialize_with = "lenient_date")]
    scheduled_for: Option<DateTime<Utc>>,
    #[serde(default, deserialize_with = "lenient_date")]
    scheduled_until: Option<DateTime<Utc>>,
    #[serde(default)]
    components: Vec<IdRef>,
    #[serde(default)]
    incident_updates: Vec<RawUpdate>,
}

#[derive(Deserialize)]
struct IdRef {
    id: String,
}

#[derive(Deserialize)]
struct RawUpdate {
    #[serde(default)]
    body: Option<String>,
    #[serde(default, deserialize_with = "lenient_date")]
    created_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum ComponentStatus {
    Operational,
    UnderMaintenance,
    DegradedPerformance,
    PartialOutage,
    MajorOutage,
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum Status {
    Investigating,
    Identified,
    Monitoring,
    Resolved,
    Postmortem,
    Scheduled,
    InProgress,
    Verifying,
    Completed,
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum Impact {
    None,
    Minor,
    Major,
    Critical,
    Maintenance,
    #[serde(other)]
    Unknown,
}

pub fn parse_statuspage(body: &[u8]) -> Result<ParsedFeed, FeedError> {
    let summary: Summary = decode(body)?;
    let mut unknown = Vec::new();

    let components = summary
        .components
        .into_iter()
        .map(|c| FeedComponent {
            level: component_level(c.status, &mut unknown),
            id: c.id,
            name: to_plain(&c.name, NAME_MAX_CHARS),
            detail: None,
            children: c.components,
        })
        .collect();

    let incidents = summary
        .incidents
        .into_iter()
        .filter_map(|raw| incident(raw, &mut unknown))
        .collect();

    let maintenances = summary
        .scheduled_maintenances
        .into_iter()
        .filter_map(|raw| maintenance(raw, &mut unknown))
        .collect();

    Ok(ParsedFeed {
        format: FeedFormat::Statuspage,
        components,
        incidents,
        maintenances,
        unknown,
    })
}

fn component_level(
    status: Option<ComponentStatus>, unknown: &mut Vec<&'static str>,
) -> HealthLevel {
    match status {
        Some(ComponentStatus::Operational) => HealthLevel::Operational,
        Some(ComponentStatus::UnderMaintenance) => HealthLevel::Maintenance,
        Some(ComponentStatus::DegradedPerformance) => HealthLevel::Degraded,
        Some(ComponentStatus::PartialOutage) => HealthLevel::PartialOutage,
        Some(ComponentStatus::MajorOutage) => HealthLevel::MajorOutage,
        Some(ComponentStatus::Unknown) | None => {
            note_unknown(unknown, "component status");
            HealthLevel::Operational
        }
    }
}

fn impact_level(impact: Option<Impact>, unknown: &mut Vec<&'static str>) -> HealthLevel {
    match impact {
        Some(Impact::None) => HealthLevel::Operational,
        Some(Impact::Minor) => HealthLevel::Degraded,
        Some(Impact::Major) => HealthLevel::PartialOutage,
        Some(Impact::Critical) => HealthLevel::MajorOutage,
        Some(Impact::Maintenance) => HealthLevel::Maintenance,
        Some(Impact::Unknown) | None => {
            note_unknown(unknown, "incident impact");
            HealthLevel::Operational
        }
    }
}

fn incident(raw: RawIncident, unknown: &mut Vec<&'static str>) -> Option<FeedIncident> {
    let phase = match raw.status {
        Some(Status::Resolved | Status::Postmortem) => return None,
        Some(Status::Investigating) => "Investigating",
        Some(Status::Identified) => "Identified",
        Some(Status::Monitoring) => "Monitoring a fix",
        Some(Status::Unknown) | None => {
            note_unknown(unknown, "incident status");
            "Ongoing"
        }
        Some(Status::Scheduled | Status::InProgress | Status::Verifying | Status::Completed) => {
            "Ongoing"
        }
    };
    let newest = raw
        .incident_updates
        .iter()
        .filter(|u| u.body.is_some())
        .max_by_key(|u| u.created_at);

    Some(FeedIncident {
        level: impact_level(raw.impact, unknown),
        phase,
        latest_update: newest
            .and_then(|u| u.body.as_deref())
            .map(|body| to_plain(body, UPDATE_MAX_CHARS))
            .filter(|text| !text.is_empty()),
        updated_at: raw.updated_at.or_else(|| {
            raw.incident_updates
                .iter()
                .filter_map(|u| u.created_at)
                .max()
        }),
        id: raw.id,
        name: to_plain(&raw.name, NAME_MAX_CHARS),
        shortlink: raw.shortlink,
        started_at: raw.started_at,
        components: raw.components.into_iter().map(|c| c.id).collect(),
    })
}

fn maintenance(raw: RawIncident, unknown: &mut Vec<&'static str>) -> Option<FeedMaintenance> {
    let in_progress = match raw.status {
        Some(Status::InProgress) => true,
        Some(Status::Scheduled) => false,
        Some(Status::Verifying | Status::Completed) => return None,
        _ => {
            note_unknown(unknown, "maintenance status");
            return None;
        }
    };

    Some(FeedMaintenance {
        id: raw.id,
        name: to_plain(&raw.name, NAME_MAX_CHARS),
        in_progress,
        shortlink: raw.shortlink,
        starts_at: raw.scheduled_for.or(raw.started_at),
        ends_at: raw.scheduled_until,
        components: raw.components.into_iter().map(|c| c.id).collect(),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn feed(value: serde_json::Value) -> ParsedFeed {
        parse_statuspage(value.to_string().as_bytes()).unwrap()
    }

    #[test]
    fn component_statuses_map_in_severity_order() {
        let parsed = feed(json!({"components": [
            {"id": "a", "name": "A", "status": "operational"},
            {"id": "b", "name": "B", "status": "under_maintenance"},
            {"id": "c", "name": "C", "status": "degraded_performance"},
            {"id": "d", "name": "D", "status": "partial_outage"},
            {"id": "e", "name": "E", "status": "major_outage"},
        ]}));

        let levels: Vec<_> = parsed.components.iter().map(|c| c.level).collect();

        assert_eq!(
            levels,
            [
                HealthLevel::Operational,
                HealthLevel::Maintenance,
                HealthLevel::Degraded,
                HealthLevel::PartialOutage,
                HealthLevel::MajorOutage,
            ]
        );
        assert!(parsed.unknown.is_empty());
    }

    #[test]
    fn incident_impacts_map_and_resolved_ones_are_excluded() {
        let incident = |id: &str, status: &str, impact: &str| json!({"id": id, "name": id, "status": status, "impact": impact});
        let parsed = feed(json!({"components": [], "incidents": [
            incident("none", "investigating", "none"),
            incident("minor", "identified", "minor"),
            incident("major", "investigating", "major"),
            incident("critical", "investigating", "critical"),
            incident("maint", "investigating", "maintenance"),
            incident("done", "resolved", "critical"),
            incident("post", "postmortem", "critical"),
            incident("watch", "monitoring", "major"),
        ]}));

        let seen: Vec<_> = parsed
            .incidents
            .iter()
            .map(|i| (i.id.as_str(), i.level, i.phase))
            .collect();

        assert_eq!(
            seen,
            [
                ("none", HealthLevel::Operational, "Investigating"),
                ("minor", HealthLevel::Degraded, "Identified"),
                ("major", HealthLevel::PartialOutage, "Investigating"),
                ("critical", HealthLevel::MajorOutage, "Investigating"),
                ("maint", HealthLevel::Maintenance, "Investigating"),
                ("watch", HealthLevel::PartialOutage, "Monitoring a fix"),
            ]
        );
    }

    #[test]
    fn unknown_strings_are_recorded_once_and_ignored() {
        let parsed = feed(json!({
            "components": [
                {"id": "a", "name": "A", "status": "teleporting"},
                {"id": "b", "name": "B", "status": "warp"},
            ],
            "incidents": [
                {"id": "i", "name": "I", "status": "pondering", "impact": "galactic"},
            ],
            "scheduled_maintenances": [
                {"id": "m", "name": "M", "status": "levitating"},
            ],
        }));

        assert_eq!(
            parsed.unknown,
            [
                "component status",
                "incident status",
                "incident impact",
                "maintenance status"
            ]
        );
        assert!(parsed
            .components
            .iter()
            .all(|c| c.level == HealthLevel::Operational));
        assert_eq!(parsed.incidents[0].level, HealthLevel::Operational);
        assert!(parsed.maintenances.is_empty());
    }

    #[test]
    fn latest_update_is_the_newest_body_in_plain_text() {
        let parsed = feed(json!({"components": [], "incidents": [{
            "id": "i", "name": "I &amp; J", "status": "investigating", "impact": "minor",
            "incident_updates": [
                {"body": "old<br />text", "created_at": "2026-10-22T10:00:00.000Z"},
                {"body": "new<br /><br />text", "created_at": "2026-10-22T12:00:00.000Z"},
            ],
        }]}));

        let incident = &parsed.incidents[0];

        assert_eq!(incident.latest_update.as_deref(), Some("new\ntext"));
        assert_eq!(incident.name, "I & J");
        assert_eq!(
            incident.updated_at,
            DateTime::parse_from_rfc3339("2026-10-22T12:00:00Z")
                .ok()
                .map(|d| d.to_utc())
        );
    }

    #[test]
    fn payload_without_components_is_rejected() {
        assert!(matches!(
            parse_statuspage(b"{}"),
            Err(FeedError::Invalid(_))
        ));
        assert!(matches!(
            parse_statuspage(b"<html></html>"),
            Err(FeedError::Invalid(_))
        ));
    }
}
