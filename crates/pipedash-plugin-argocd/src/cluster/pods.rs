use serde::Deserialize;

use crate::types::logs::{
    ApplicationTree,
    OwnerReference,
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

fn managed(resources: &[ResourceStatus], kind: &str, namespace: &str, name: &str) -> bool {
    resources.iter().any(|resource| {
        resource.kind == kind
            && resource.name == name
            && resource.namespace.as_deref().unwrap_or_default() == namespace
    })
}

fn owned_by_managed(
    resources: &[ResourceStatus], namespace: &str, owner: &OwnerReference, parents: &[Owned],
) -> bool {
    managed(resources, &owner.kind, namespace, &owner.name)
        || parents
            .iter()
            .filter(|parent| {
                parent.metadata.namespace == namespace && parent.metadata.name == owner.name
            })
            .flat_map(|parent| &parent.metadata.owner_references)
            .any(|grand| managed(resources, &grand.kind, namespace, &grand.name))
}

pub(crate) fn owned_pods(
    resources: &[ResourceStatus], pods: Vec<Pod>, replica_sets: &[Owned], jobs: &[Owned],
) -> Vec<Pod> {
    pods.into_iter()
        .filter(|pod| {
            let meta = &pod.metadata;
            managed(resources, "Pod", &meta.namespace, &meta.name)
                || meta.owner_references.iter().any(|owner| {
                    let parents = match owner.kind.as_str() {
                        "ReplicaSet" => replica_sets,
                        "Job" => jobs,
                        _ => &[],
                    };
                    owned_by_managed(resources, &meta.namespace, owner, parents)
                })
        })
        .collect()
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
        let known = nodes[..listed].iter().any(|node| {
            node.kind == "Pod"
                && node.group.is_empty()
                && node.namespace == *namespace
                && node.name == *name
        });
        if !known {
            nodes.push(ResourceNode {
                group: String::new(),
                kind: "Pod".to_string(),
                namespace: namespace.clone(),
                name: name.clone(),
                health: Some(pod_health(pod)),
                info: Vec::new(),
            });
        }
    }
    ApplicationTree { nodes }
}

#[cfg(test)]
mod tests;
