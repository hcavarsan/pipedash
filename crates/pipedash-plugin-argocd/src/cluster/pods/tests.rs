use serde_json::json;

use super::*;

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
fn tree_reports_the_live_health_of_every_fetched_pod() {
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
            ("", "Pod", "lonely", Some("Unknown")),
            ("", "Pod", "api-7f-a", Some("Unknown")),
        ]
    );
}
