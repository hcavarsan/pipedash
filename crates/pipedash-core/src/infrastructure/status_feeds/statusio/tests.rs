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
