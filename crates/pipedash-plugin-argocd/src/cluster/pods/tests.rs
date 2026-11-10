use serde_json::json;

use super::*;

fn resources(entries: &[(&str, &str, &str)]) -> Vec<ResourceStatus> {
    entries
        .iter()
        .map(|(kind, namespace, name)| {
            serde_json::from_value(json!({"kind": kind, "namespace": namespace, "name": name}))
                .unwrap()
        })
        .collect()
}

fn pod(name: &str, owner: Option<(&str, &str)>) -> Pod {
    let owners: Vec<_> = owner
        .map(|(kind, name)| json!({"kind": kind, "name": name}))
        .into_iter()
        .collect();
    serde_json::from_value(json!({
        "metadata": {"name": name, "namespace": "web", "ownerReferences": owners},
    }))
    .unwrap()
}

fn owned(name: &str, owner: (&str, &str)) -> Owned {
    serde_json::from_value(json!({
        "metadata": {
            "name": name,
            "namespace": "web",
            "ownerReferences": [{"kind": owner.0, "name": owner.1}],
        },
    }))
    .unwrap()
}

fn names(pods: &[Pod]) -> Vec<&str> {
    pods.iter().map(|pod| pod.metadata.name.as_str()).collect()
}

#[test]
fn deployment_pods_are_found_through_their_replica_set() {
    let managed = resources(&[("Deployment", "web", "api")]);
    let replica_sets = [
        owned("api-7f", ("Deployment", "api")),
        owned("db-1", ("Deployment", "db")),
    ];
    let pods = vec![
        pod("api-7f-a", Some(("ReplicaSet", "api-7f"))),
        pod("db-1-a", Some(("ReplicaSet", "db-1"))),
    ];

    let found = owned_pods(&managed, pods, &replica_sets, &[]);

    assert_eq!(names(&found), ["api-7f-a"]);
}

#[test]
fn cron_job_pods_are_found_through_their_job() {
    let managed = resources(&[("CronJob", "web", "nightly")]);
    let jobs = [owned("nightly-28", ("CronJob", "nightly"))];
    let pods = vec![
        pod("nightly-28-x", Some(("Job", "nightly-28"))),
        pod("other-1-x", Some(("Job", "other-1"))),
    ];

    let found = owned_pods(&managed, pods, &[], &jobs);

    assert_eq!(names(&found), ["nightly-28-x"]);
}

#[test]
fn stateful_set_and_bare_pods_are_owned_directly() {
    let managed = resources(&[("StatefulSet", "web", "db"), ("Pod", "web", "lonely")]);
    let pods = vec![
        pod("db-0", Some(("StatefulSet", "db"))),
        pod("lonely", None),
        pod("stranger", None),
    ];

    let found = owned_pods(&managed, pods, &[], &[]);

    assert_eq!(names(&found), ["db-0", "lonely"]);
}

#[test]
fn unrelated_pods_in_the_same_namespace_are_excluded() {
    let managed = resources(&[("Deployment", "web", "api")]);
    let replica_sets = [owned("api-7f", ("Deployment", "api"))];
    let pods = vec![
        pod("stranger", None),
        pod("elsewhere", Some(("ReplicaSet", "unknown"))),
        pod("same-name", Some(("Deployment", "other"))),
    ];

    assert!(owned_pods(&managed, pods, &replica_sets, &[]).is_empty());
}

#[test]
fn resources_in_another_namespace_do_not_own_pods() {
    let managed = resources(&[("StatefulSet", "other", "db")]);

    assert!(owned_pods(
        &managed,
        vec![pod("db-0", Some(("StatefulSet", "db")))],
        &[],
        &[]
    )
    .is_empty());
}

fn health_of(pod: serde_json::Value) -> (String, Option<String>) {
    let pod: Pod = serde_json::from_value(pod).unwrap();
    let health = pod_health(&pod);
    (health.status, health.message)
}

#[test]
fn pod_health_follows_phase_and_readiness() {
    let cases = [
        (json!({"status": {"phase": "Pending"}}), "Progressing"),
        (json!({"status": {"phase": "Failed"}}), "Degraded"),
        (json!({"status": {"phase": "Succeeded"}}), "Healthy"),
        (json!({"status": {"phase": "Unknown"}}), "Unknown"),
        (
            json!({"status": {"phase": "Running", "containerStatuses": [{"name": "a", "ready": true}]}}),
            "Healthy",
        ),
        (
            json!({"status": {"phase": "Running", "containerStatuses": [
                {"name": "a", "ready": true}, {"name": "b", "ready": false}]}}),
            "Progressing",
        ),
    ];
    for (pod, expected) in cases {
        assert_eq!(
            health_of(pod.clone()),
            (expected.to_string(), None),
            "{pod}"
        );
    }
}

#[test]
fn a_container_stuck_waiting_degrades_the_pod_with_its_reason() {
    let crash = json!({"status": {"phase": "Running", "containerStatuses": [
        {"name": "app", "ready": false, "state": {"waiting": {"reason": "CrashLoopBackOff"}}}]}});
    let init = json!({"status": {"phase": "Pending", "initContainerStatuses": [
        {"name": "migrate", "state": {"waiting": {"reason": "ImagePullBackOff"}}}]}});

    assert_eq!(
        health_of(crash),
        (
            "Degraded".to_string(),
            Some("app: CrashLoopBackOff".to_string())
        )
    );
    assert_eq!(
        health_of(init),
        (
            "Degraded".to_string(),
            Some("migrate: ImagePullBackOff".to_string())
        )
    );
}

#[test]
fn a_container_that_is_still_starting_is_not_degraded() {
    for reason in ["ContainerCreating", "PodInitializing", ""] {
        let pod = json!({"status": {"phase": "Pending", "containerStatuses": [
            {"name": "app", "state": {"waiting": {"reason": reason}}}]}});

        assert_eq!(
            health_of(pod),
            ("Progressing".to_string(), None),
            "{reason:?}"
        );
    }
}

#[test]
fn only_the_in_cluster_destination_has_pods() {
    let destination = |value| serde_json::from_value::<ApplicationDestination>(value).unwrap();

    assert!(in_cluster(&destination(
        json!({"server": "https://kubernetes.default.svc"})
    )));
    assert!(in_cluster(&destination(json!({"name": "in-cluster"}))));
    assert!(!in_cluster(&destination(
        json!({"server": "https://10.0.0.1:6443"})
    )));
    assert!(!in_cluster(&destination(json!({"name": "prod"}))));
    assert!(!in_cluster(&destination(json!({}))));
}

#[test]
fn tree_lists_managed_resources_and_adds_pods_it_does_not_know() {
    let app: Application = serde_json::from_value(json!({
        "metadata": {"name": "web"},
        "spec": {"destination": {}, "project": "default"},
        "status": {"resources": [
            {"group": "apps", "kind": "Deployment", "namespace": "web", "name": "api",
                "health": {"status": "Progressing"}},
            {"kind": "Pod", "namespace": "web", "name": "lonely", "health": {"status": "Healthy"}},
        ]},
    }))
    .unwrap();
    let pods = vec![
        pod("lonely", None),
        pod("api-7f-a", Some(("ReplicaSet", "api-7f"))),
    ];

    let tree = tree(&app, &pods);

    let summary: Vec<_> = tree
        .nodes
        .iter()
        .map(|node| {
            (
                node.group.as_str(),
                node.kind.as_str(),
                node.name.as_str(),
                node.health.as_ref().map(|health| health.status.as_str()),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("apps", "Deployment", "api", Some("Progressing")),
            ("", "Pod", "lonely", Some("Healthy")),
            ("", "Pod", "api-7f-a", Some("Unknown")),
        ]
    );
}
