use pipedash_plugin_api::{
    ComponentHealth,
    HealthLevel,
};
use serde::Deserialize;

#[derive(Default, Deserialize)]
pub(crate) struct Deployment {
    #[serde(default)]
    spec: DeploymentSpec,
    #[serde(default)]
    status: DeploymentStatus,
}

#[derive(Default, Deserialize)]
struct DeploymentSpec {
    replicas: Option<i64>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeploymentStatus {
    available_replicas: Option<i64>,
    #[serde(default)]
    conditions: Vec<DeploymentCondition>,
}

#[derive(Deserialize)]
struct DeploymentCondition {
    #[serde(rename = "type")]
    kind: String,
    status: String,
}

impl Deployment {
    fn available_condition_is_false(&self) -> bool {
        self.status
            .conditions
            .iter()
            .any(|condition| condition.kind == "Available" && condition.status == "False")
    }

    pub fn health(&self, name: &str) -> Option<ComponentHealth> {
        let desired = self.spec.replicas;
        let available = self.status.available_replicas;
        let (level, detail) = match (desired, available) {
            (Some(0), _) => (HealthLevel::MajorOutage, "scaled to zero".to_string()),
            (_, Some(0)) => (HealthLevel::MajorOutage, "not running".to_string()),
            (Some(desired), Some(available)) if available < desired => (
                HealthLevel::Degraded,
                format!("{available}/{desired} replicas available"),
            ),
            (Some(_), Some(_)) => return None,
            _ if self.available_condition_is_false() => {
                (HealthLevel::MajorOutage, "not available".to_string())
            }
            _ => return None,
        };

        Some(ComponentHealth {
            name: name.to_string(),
            level,
            detail: Some(detail),
        })
    }
}

#[cfg(test)]
mod tests {
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
}
