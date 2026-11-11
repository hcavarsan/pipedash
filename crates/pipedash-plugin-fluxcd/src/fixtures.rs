use pipedash_plugin_api::{
    ArtifactObservation,
    ClusterId,
    Pipeline,
    PipelineRun,
    RepoId,
};
use serde_json::{
    json,
    Value,
};

use crate::mapper::correlation::{
    self,
    Facts,
};
use crate::mapper::{
    build_runs,
    map_pipeline,
};
use crate::pods::{
    self,
    Live,
    LiveObject,
};
use crate::types::{
    FluxResource,
    Kind,
};

const COMMIT: &str = "6b7aab8a1f0c4e5d9a3b2c1d0e9f8a7b6c5d4e3f";
const DIGEST: &str = "sha256:0f5a7c1e9b3d2468ace02468ace02468ace02468ace02468ace02468ace02460";
const CLUSTER_UID: &str = "5c1f9f1e-4a3b-4c2d-8e7f-0a1b2c3d4e5f";

fn ready(revision: &str, at: &str) -> Value {
    json!({
        "type": "Ready",
        "status": "True",
        "reason": "ReconciliationSucceeded",
        "message": format!("Applied revision: {revision}"),
        "observedGeneration": 1,
        "lastTransitionTime": at,
    })
}

fn resource(kind: Kind, spec: Value, status: Value) -> Option<FluxResource> {
    FluxResource::from_value(
        kind,
        json!({
            "metadata": {
                "name": "shop",
                "namespace": "flux-system",
                "uid": "9a1b2c3d-0000-4000-8000-000000000001",
                "generation": 1,
                "creationTimestamp": "2026-10-20T10:00:00Z",
                "labels": { "app.kubernetes.io/part-of": "shop" },
            },
            "spec": spec,
            "status": status,
        }),
    )
    .ok()
}

fn pod(name: &str, owner: Option<(&str, &str)>) -> Value {
    let owners: Vec<Value> = owner
        .map(|(kind, name)| json!({ "kind": kind, "name": name }))
        .into_iter()
        .collect();

    json!({
        "metadata": { "name": name, "namespace": "shop", "ownerReferences": owners },
        "status": {
            "containerStatuses": [{
                "image": "registry.example.test/shop/api:1.4.2",
                "imageID": format!("registry.example.test/shop/api@{DIGEST}"),
            }],
        },
    })
}

fn objects(values: Vec<Value>) -> Option<Vec<LiveObject>> {
    serde_json::from_value(Value::Array(values)).ok()
}

fn facts(images: Vec<ArtifactObservation>) -> Facts {
    Facts {
        repo: RepoId::parse("https://git.example.test/acme/fleet.git"),
        cluster: ClusterId::from_uid(CLUSTER_UID),
        images,
    }
}

fn kustomization() -> Option<(FluxResource, Facts)> {
    let revision = format!("main@sha1:{COMMIT}");
    let resource = resource(
        Kind::Kustomization,
        json!({
            "path": "./apps/shop",
            "sourceRef": { "kind": "GitRepository", "name": "fleet" },
            "targetNamespace": "shop",
            "dependsOn": [{ "name": "infra" }],
        }),
        json!({
            "observedGeneration": 1,
            "lastAttemptedRevision": revision,
            "lastAppliedRevision": revision,
            "conditions": [ready(&revision, "2026-10-21T10:00:05Z")],
            "inventory": { "entries": [
                { "id": "shop_api_apps_Deployment", "v": "v1" },
                { "id": "shop_api__Service", "v": "v1" },
            ] },
            "history": [{
                "digest": "sha256:aa",
                "firstReconciled": "2026-10-21T10:00:00Z",
                "lastReconciled": "2026-10-21T10:00:05Z",
                "lastReconciledDuration": "5s",
                "lastReconciledStatus": "ReconciliationSucceeded",
                "totalReconciliations": 2,
                "metadata": { "revision": revision },
            }],
        }),
    )?;
    let live = Live {
        pods: objects(vec![
            pod("api-7f-a", Some(("ReplicaSet", "api-7f"))),
            pod("stranger", None),
        ])?,
        replica_sets: objects(vec![json!({
            "metadata": {
                "name": "api-7f",
                "namespace": "shop",
                "ownerReferences": [{ "kind": "Deployment", "name": "api" }],
            },
        })])?,
    };
    let images = pods::images(pods::owned_pods(&pods::workloads(&resource), &[&live]));

    Some((resource, facts(images)))
}

fn helm_release() -> Option<(FluxResource, Facts)> {
    let resource = resource(
        Kind::HelmRelease,
        json!({
            "targetNamespace": "shop",
            "chart": { "spec": {
                "chart": "./charts/shop",
                "sourceRef": { "kind": "GitRepository", "name": "fleet" },
            } },
        }),
        json!({
            "observedGeneration": 1,
            "lastAttemptedRevision": "0.1.0",
            "lastAttemptedReleaseAction": "upgrade",
            "lastAttemptedReleaseActionDuration": "12s",
            "conditions": [ready("0.1.0", "2026-10-21T11:00:12Z")],
            "history": [{
                "version": 3,
                "action": "upgrade",
                "status": "deployed",
                "chartName": "shop",
                "chartVersion": "0.1.0",
                "digest": "sha256:bb",
                "firstDeployed": "2026-10-20T10:00:00Z",
                "lastDeployed": "2026-10-21T11:00:00Z",
            }],
        }),
    )?;
    let images = pods::images(&objects(vec![pod("shop-0", None)])?);

    Some((resource, facts(images)))
}

fn pair((resource, facts): (FluxResource, Facts)) -> Option<(Pipeline, Vec<PipelineRun>)> {
    let mut pipeline = map_pipeline(&resource, &[]);
    pipeline.correlation = correlation::pipeline(&resource, &facts);
    let mut runs = build_runs(&resource, &[]).ok()?;
    correlation::attach_runs(&mut runs, &resource, &facts);

    Some((pipeline, runs))
}

pub(crate) fn all() -> Vec<(Pipeline, Vec<PipelineRun>)> {
    [kustomization(), helm_release()]
        .into_iter()
        .flatten()
        .filter_map(pair)
        .collect()
}

#[cfg(test)]
mod tests;
