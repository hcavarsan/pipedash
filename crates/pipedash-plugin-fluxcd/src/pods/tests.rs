use serde_json::{
    json,
    Value,
};

use super::*;
use crate::types::Kind;

const FIRST: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const SECOND: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";

fn kustomization(entries: &[&str]) -> FluxResource {
    let entries: Vec<Value> = entries
        .iter()
        .map(|id| json!({ "id": id, "v": "v1" }))
        .collect();

    FluxResource::from_value(
        Kind::Kustomization,
        json!({
            "metadata": { "name": "shop", "namespace": "flux-system" },
            "status": { "inventory": { "entries": entries } },
        }),
    )
    .unwrap()
}

fn objects(values: Vec<Value>) -> Vec<LiveObject> {
    serde_json::from_value(Value::Array(values)).unwrap()
}

fn object(name: &str, owner: Option<(&str, &str)>) -> Value {
    let owners: Vec<Value> = owner
        .map(|(kind, name)| json!({ "kind": kind, "name": name }))
        .into_iter()
        .collect();

    json!({ "metadata": { "name": name, "namespace": "shop", "ownerReferences": owners } })
}

fn pod_with(name: &str, owner: Option<(&str, &str)>, containers: Value) -> Value {
    let mut pod = object(name, owner);
    pod["status"] = json!({ "containerStatuses": containers });

    pod
}

fn names<'a>(pods: &'a [&LiveObject]) -> Vec<&'a str> {
    pods.iter().map(|pod| pod.metadata.name.as_str()).collect()
}

fn observation(image: &str, digest: Option<&str>) -> ArtifactObservation {
    let mut artifact = ImageRef::parse(image).unwrap().into_observation();
    artifact.digest = digest.and_then(Digest::parse);

    artifact
}

#[test]
fn inventory_keeps_only_valid_workloads_of_the_apps_group() {
    let resource = kustomization(&[
        "shop_api_apps_Deployment",
        "shop_db_apps_StatefulSet",
        "other_agent_apps_DaemonSet",
        "shop_api__Service",
        "shop_nightly_batch_Job",
        "_admin_rbac.authorization.k8s.io_ClusterRole",
        "Bad_api_apps_Deployment",
        "shop_api_apps_Deployment_extra",
        "malformed",
    ]);

    let workloads = workloads(&resource);

    assert_eq!(
        workloads,
        vec![
            Workload {
                kind: WorkloadKind::Deployment,
                namespace: "shop".to_string(),
                name: "api".to_string(),
            },
            Workload {
                kind: WorkloadKind::StatefulSet,
                namespace: "shop".to_string(),
                name: "db".to_string(),
            },
            Workload {
                kind: WorkloadKind::DaemonSet,
                namespace: "other".to_string(),
                name: "agent".to_string(),
            },
        ]
    );
}

#[test]
fn pods_are_owned_through_replica_sets_stateful_sets_and_daemon_sets() {
    let resource = kustomization(&[
        "shop_api_apps_Deployment",
        "shop_db_apps_StatefulSet",
        "shop_agent_apps_DaemonSet",
    ]);
    let live = Live {
        pods: objects(vec![
            object("api-7f-a", Some(("ReplicaSet", "api-7f"))),
            object("db-0", Some(("StatefulSet", "db"))),
            object("agent-x", Some(("DaemonSet", "agent"))),
            object("other-1-a", Some(("ReplicaSet", "other-1"))),
            object("stranger", None),
        ]),
        replica_sets: objects(vec![
            object("api-7f", Some(("Deployment", "api"))),
            object("other-1", Some(("Deployment", "other"))),
        ]),
    };

    let found = owned_pods(&workloads(&resource), &[&live]);

    assert_eq!(names(&found), ["api-7f-a", "db-0", "agent-x"]);
}

#[test]
fn an_empty_inventory_owns_no_pods() {
    let live = Live {
        pods: objects(vec![object("stranger", None)]),
        replica_sets: Vec::new(),
    };

    assert!(owned_pods(&workloads(&kustomization(&[])), &[&live]).is_empty());
}

#[test]
fn images_bind_the_digest_of_the_image_id_and_are_sorted_and_deduplicated() {
    let pods = objects(vec![
        pod_with(
            "b",
            None,
            json!([
                { "image": "registry.example.test/shop/web:2.0", "imageID": format!("registry.example.test/shop/web@{SECOND}") },
                { "image": "registry.example.test/shop/api:1.4.2", "imageID": format!("docker-pullable://registry.example.test/shop/api@{FIRST}") },
            ]),
        ),
        pod_with(
            "a",
            None,
            json!([
                { "image": "registry.example.test/shop/api:1.4.2", "imageID": format!("docker-pullable://registry.example.test/shop/api@{FIRST}") },
                { "image": "", "imageID": "" },
            ]),
        ),
    ]);

    let images = images(&pods);

    assert_eq!(
        images,
        vec![
            observation("registry.example.test/shop/api:1.4.2", Some(FIRST)),
            observation("registry.example.test/shop/web:2.0", Some(SECOND)),
        ]
    );
}

#[test]
fn init_containers_count_and_a_missing_image_id_keeps_the_digest_of_the_reference() {
    let mut pod = pod_with("a", None, json!([]));
    pod["status"]["initContainerStatuses"] = json!([
        { "image": format!("registry.example.test/shop/migrate@{FIRST}"), "imageID": "" },
        { "image": "registry.example.test/shop/seed:1", "imageID": "" },
    ]);

    let images = images(&objects(vec![pod]));

    assert_eq!(
        images,
        vec![
            observation("registry.example.test/shop/migrate", Some(FIRST)),
            observation("registry.example.test/shop/seed:1", None),
        ]
    );
}
