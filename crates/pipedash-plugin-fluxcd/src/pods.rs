use pipedash_plugin_api::kubernetes::{
    self,
    ObjectKey,
    OwnedObject,
    Owners,
};
use pipedash_plugin_api::{
    ArtifactObservation,
    Digest,
    ImageRef,
};
use serde::Deserialize;

use crate::types::FluxResource;
use crate::workloads::WorkloadKind;

const WORKLOAD_GROUP: &str = "apps";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Owner {
    kind: String,
    name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Meta {
    name: String,
    namespace: String,
    owner_references: Vec<Owner>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ContainerStatus {
    image: String,
    #[serde(rename = "imageID")]
    image_id: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Status {
    init_container_statuses: Vec<ContainerStatus>,
    container_statuses: Vec<ContainerStatus>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct LiveObject {
    metadata: Meta,
    status: Status,
}

impl OwnedObject for LiveObject {
    fn namespace(&self) -> &str {
        &self.metadata.namespace
    }

    fn name(&self) -> &str {
        &self.metadata.name
    }

    fn owner_references(&self) -> impl Iterator<Item = (&str, &str)> {
        self.metadata
            .owner_references
            .iter()
            .map(|owner| (owner.kind.as_str(), owner.name.as_str()))
    }
}

#[derive(Debug, Default)]
pub(crate) struct Live {
    pub pods: Vec<LiveObject>,
    pub replica_sets: Vec<LiveObject>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Workload {
    pub kind: WorkloadKind,
    pub namespace: String,
    pub name: String,
}

impl Workload {
    fn from_inventory(id: &str) -> Option<Self> {
        let mut parts = id.split('_');
        let (namespace, name, group, kind) =
            (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
        let valid = parts.next().is_none()
            && group == WORKLOAD_GROUP
            && kubernetes::is_namespace(namespace);
        if !valid {
            return None;
        }

        Some(Self {
            kind: WorkloadKind::ALL
                .into_iter()
                .find(|candidate| candidate.as_str() == kind)?,
            namespace: namespace.to_string(),
            name: name.to_string(),
        })
    }

    fn key(&self) -> ObjectKey<'_> {
        ObjectKey {
            kind: self.kind.as_str(),
            namespace: &self.namespace,
            name: &self.name,
        }
    }
}

pub(crate) fn workloads(resource: &FluxResource) -> Vec<Workload> {
    resource
        .status
        .inventory
        .entries
        .iter()
        .filter_map(|entry| Workload::from_inventory(&entry.id))
        .collect()
}

pub(crate) fn owned_pods<'a>(workloads: &[Workload], live: &[&'a Live]) -> Vec<&'a LiveObject> {
    let managed: Vec<ObjectKey<'_>> = workloads.iter().map(Workload::key).collect();
    let replica_sets: Vec<&LiveObject> = live
        .iter()
        .copied()
        .flat_map(|live| &live.replica_sets)
        .collect();

    kubernetes::owned_by(
        live.iter().copied().flat_map(|live| &live.pods),
        &Owners {
            managed: &managed,
            parents: &[("ReplicaSet", replica_sets.as_slice())],
        },
    )
}

impl ContainerStatus {
    fn artifact(&self) -> Option<ArtifactObservation> {
        let mut artifact = ImageRef::parse(&self.image)?.into_observation();
        artifact.digest = Digest::from_image_id(&self.image_id).or(artifact.digest);

        Some(artifact)
    }
}

pub(crate) fn images<'a>(
    pods: impl IntoIterator<Item = &'a LiveObject>,
) -> Vec<ArtifactObservation> {
    let mut images: Vec<ArtifactObservation> = pods
        .into_iter()
        .flat_map(|pod| {
            pod.status
                .init_container_statuses
                .iter()
                .chain(&pod.status.container_statuses)
        })
        .filter_map(ContainerStatus::artifact)
        .collect();
    images.sort_by(|a, b| {
        (&a.repository, &a.digest, &a.tag).cmp(&(&b.repository, &b.digest, &b.tag))
    });
    images.dedup();

    images
}

#[cfg(test)]
mod tests;
