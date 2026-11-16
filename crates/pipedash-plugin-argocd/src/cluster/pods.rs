use pipedash_plugin_api::kubernetes::{
    ObjectKey,
    OwnedObject,
};
use serde::Deserialize;

use crate::types::logs::{
    ApplicationTree,
    Pod,
    PodMetadata,
    ResourceNode,
};
use crate::types::{
    Application,
    ApplicationDestination,
    HealthStatus,
    ResourceStatus,
};

const IN_CLUSTER_SERVER: &str = "https://kubernetes.default.svc";
const IN_CLUSTER_NAME: &str = "in-cluster";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct Owned {
    pub metadata: PodMetadata,
}

pub(crate) fn in_cluster(destination: &ApplicationDestination) -> bool {
    destination.server.as_deref() == Some(IN_CLUSTER_SERVER)
        || destination.name.as_deref() == Some(IN_CLUSTER_NAME)
}

pub(crate) fn managed(resources: &[ResourceStatus]) -> Vec<ObjectKey<'_>> {
    resources
        .iter()
        .map(|resource| ObjectKey {
            kind: &resource.kind,
            namespace: resource.namespace.as_deref().unwrap_or_default(),
            name: &resource.name,
        })
        .collect()
}

fn references(metadata: &PodMetadata) -> impl Iterator<Item = (&str, &str)> {
    metadata
        .owner_references
        .iter()
        .map(|owner| (owner.kind.as_str(), owner.name.as_str()))
}

impl OwnedObject for Pod {
    fn namespace(&self) -> &str {
        &self.metadata.namespace
    }

    fn name(&self) -> &str {
        &self.metadata.name
    }

    fn owner_references(&self) -> impl Iterator<Item = (&str, &str)> {
        references(&self.metadata)
    }
}

impl OwnedObject for Owned {
    fn namespace(&self) -> &str {
        &self.metadata.namespace
    }

    fn name(&self) -> &str {
        &self.metadata.name
    }

    fn owner_references(&self) -> impl Iterator<Item = (&str, &str)> {
        references(&self.metadata)
    }
}

fn failing_container(pod: &Pod) -> Option<String> {
    pod.status
        .init_container_statuses
        .iter()
        .chain(&pod.status.container_statuses)
        .find_map(|status| {
            let reason = status
                .state
                .waiting
                .as_ref()?
                .reason
                .as_deref()
                .filter(|reason| {
                    !matches!(*reason, "" | "ContainerCreating" | "PodInitializing")
                })?;
            Some(format!("{}: {reason}", status.name))
        })
}

pub(crate) fn pod_health(pod: &Pod) -> HealthStatus {
    let health = |status: &str, message: Option<String>| HealthStatus {
        status: status.to_string(),
        message,
    };
    if let Some(message) = failing_container(pod) {
        return health("Degraded", Some(message));
    }

    match pod.status.phase.as_str() {
        "Failed" => health("Degraded", None),
        "Pending" => health("Progressing", None),
        "Running"
            if pod
                .status
                .container_statuses
                .iter()
                .all(|status| status.ready) =>
        {
            health("Healthy", None)
        }
        "Running" => health("Progressing", None),
        "Succeeded" => health("Healthy", None),
        _ => health("Unknown", None),
    }
}

pub(crate) fn tree(app: &Application, pods: &[Pod]) -> ApplicationTree {
    let mut nodes: Vec<ResourceNode> = app
        .status
        .resources
        .iter()
        .map(|resource| ResourceNode {
            group: resource.group.clone().unwrap_or_default(),
            kind: resource.kind.clone(),
            namespace: resource.namespace.clone().unwrap_or_default(),
            name: resource.name.clone(),
            health: resource.health.clone(),
            info: Vec::new(),
        })
        .collect();
    let listed = nodes.len();
    for pod in pods {
        let (namespace, name) = (&pod.metadata.namespace, &pod.metadata.name);
        let health = Some(pod_health(pod));
        let known = nodes[..listed].iter_mut().find(|node| {
            node.kind == "Pod"
                && node.group.is_empty()
                && node.namespace == *namespace
                && node.name == *name
        });
        match known {
            Some(node) => node.health = health,
            None => nodes.push(ResourceNode {
                group: String::new(),
                kind: "Pod".to_string(),
                namespace: namespace.clone(),
                name: name.clone(),
                health,
                info: Vec::new(),
            }),
        }
    }
    ApplicationTree { nodes }
}

pub(crate) fn log_tree(app: &Application, pods: &[Pod]) -> ApplicationTree {
    let mut tree = tree(app, pods);
    if !in_cluster(&app.spec.destination) {
        tree.nodes
            .retain(|node| node.kind != "Pod" || !node.group.is_empty());
    }
    tree
}

#[cfg(test)]
mod tests;
