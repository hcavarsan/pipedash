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
mod tests;
