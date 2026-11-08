use pipedash_plugin_api::HealthLevel;
use serde_json::json;

use super::*;

fn deployment(value: serde_json::Value) -> Deployment {
    serde_json::from_value(value).unwrap()
}

fn outcome(value: serde_json::Value) -> Option<(HealthLevel, String)> {
    deployment(value)
        .health("source-controller")
        .map(|health| (health.level, health.detail.unwrap_or_default()))
}

#[test]
fn replica_counts_decide_before_conditions() {
    let available_false = json!([{ "type": "Available", "status": "False" }]);

    assert_eq!(
        outcome(json!({ "spec": { "replicas": 0 }, "status": { "availableReplicas": 1 } })),
        Some((HealthLevel::MajorOutage, "scaled to zero".to_string()))
    );
    assert_eq!(
        outcome(json!({ "spec": { "replicas": 2 }, "status": { "availableReplicas": 0 } })),
        Some((HealthLevel::MajorOutage, "not running".to_string()))
    );
    assert_eq!(
        outcome(json!({
            "spec": { "replicas": 3 },
            "status": { "availableReplicas": 2, "conditions": available_false }
        })),
        Some((HealthLevel::Degraded, "2/3 replicas available".to_string()))
    );
    assert_eq!(
        outcome(json!({
            "spec": { "replicas": 3 },
            "status": { "availableReplicas": 3, "conditions": available_false }
        })),
        None
    );
}

#[test]
fn missing_counts_fall_back_to_the_available_condition() {
    assert_eq!(
        outcome(json!({
            "spec": { "replicas": 1 },
            "status": { "conditions": [{ "type": "Available", "status": "False" }] }
        }))
        .map(|(level, _)| level),
        Some(HealthLevel::MajorOutage)
    );
    assert_eq!(
        outcome(json!({
            "spec": { "replicas": 1 },
            "status": { "conditions": [{ "type": "Available", "status": "True" }] }
        })),
        None
    );
    assert_eq!(outcome(json!({})), None);
}
