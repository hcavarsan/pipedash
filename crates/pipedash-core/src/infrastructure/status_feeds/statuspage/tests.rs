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
