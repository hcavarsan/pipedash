use pipedash_plugin_api::health::deployment_health;
use pipedash_plugin_api::ComponentHealth;
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
        deployment_health(
            name,
            self.spec.replicas,
            self.status.available_replicas,
            self.available_condition_is_false(),
        )
    }
}

#[cfg(test)]
mod tests;
