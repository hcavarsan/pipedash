use serde_json::json;

use super::*;

fn workload(spec: Value, status: Value) -> Value {
    json!({
        "metadata": {
            "name": "web",
            "namespace": "apps",
            "generation": 4,
            "labels": {
                "kustomize.toolkit.fluxcd.io/name": "app",
                "kustomize.toolkit.fluxcd.io/namespace": "ci",
            },
        },
        "spec": spec,
        "status": status,
    })
}

fn found(kind: WorkloadKind, spec: Value, status: Value) -> Option<Finding> {
    finding(kind, Kind::Kustomization, workload(spec, status)).unwrap()
}

fn issue(kind: WorkloadKind, spec: Value, status: Value) -> Option<String> {
    found(kind, spec, status).map(|found| found.message)
}

fn warming_daemon_set(min_ready_seconds: u64) -> Finding {
    found(
        WorkloadKind::DaemonSet,
        json!({ "minReadySeconds": min_ready_seconds }),
        json!({
            "observedGeneration": 4, "desiredNumberScheduled": 3,
            "updatedNumberScheduled": 3, "numberUnavailable": 1,
        }),
    )
    .unwrap()
}

fn messages(findings: Vec<Finding>) -> Vec<String> {
    findings.into_iter().map(|found| found.message).collect()
}

fn progressing(status: &str, reason: &str) -> Value {
    json!({ "type": "Progressing", "status": status, "reason": reason })
}

#[test]
fn deployment_is_unhealthy_when_fully_rolled_out_but_replicas_are_not_ready() {
    let status = json!({
        "observedGeneration": 4, "replicas": 3, "updatedReplicas": 3,
        "readyReplicas": 1, "availableReplicas": 1,
        "conditions": [progressing("True", "NewReplicaSetAvailable")],
    });

    assert_eq!(
        issue(WorkloadKind::Deployment, json!({ "replicas": 3 }), status).as_deref(),
        Some("Deployment apps/web: 1 of 3 replicas ready")
    );
}

#[test]
fn deployment_with_no_ready_replicas_and_no_status_counts_is_unhealthy() {
    let status = json!({ "observedGeneration": 4, "updatedReplicas": 1 });

    assert_eq!(
        issue(WorkloadKind::Deployment, json!({}), status).as_deref(),
        Some("Deployment apps/web: 0 of 1 replicas ready")
    );
}

#[test]
fn deployment_rollouts_in_progress_are_not_unhealthy() {
    let cases = [
        json!({ "observedGeneration": 3, "updatedReplicas": 3, "readyReplicas": 0 }),
        json!({ "observedGeneration": 4, "updatedReplicas": 1, "readyReplicas": 2 }),
        json!({
            "observedGeneration": 4, "updatedReplicas": 3, "readyReplicas": 0,
            "conditions": [progressing("True", "ReplicaSetUpdated")],
        }),
    ];

    for status in cases {
        assert_eq!(
            issue(WorkloadKind::Deployment, json!({ "replicas": 3 }), status),
            None
        );
    }
}

#[test]
fn deployment_past_its_progress_deadline_is_unhealthy_even_mid_rollout() {
    let status = json!({
        "observedGeneration": 4, "updatedReplicas": 1, "readyReplicas": 3,
        "conditions": [progressing("False", "ProgressDeadlineExceeded")],
    });

    assert_eq!(
        issue(WorkloadKind::Deployment, json!({ "replicas": 3 }), status).as_deref(),
        Some("Deployment apps/web: rollout exceeded its progress deadline")
    );
}

#[test]
fn scaled_to_zero_workloads_are_healthy() {
    let status = json!({
        "observedGeneration": 4, "updatedReplicas": 0, "readyReplicas": 0,
        "currentRevision": "web-1", "updateRevision": "web-1",
        "conditions": [progressing("False", "ProgressDeadlineExceeded")],
    });

    for kind in [WorkloadKind::Deployment, WorkloadKind::StatefulSet] {
        assert_eq!(issue(kind, json!({ "replicas": 0 }), status.clone()), None);
    }
}

#[test]
fn stateful_set_is_unhealthy_only_when_no_update_is_pending() {
    let steady = json!({
        "observedGeneration": 4, "readyReplicas": 1,
        "currentRevision": "web-1", "updateRevision": "web-1",
    });
    let updating = json!({
        "observedGeneration": 4, "readyReplicas": 1,
        "currentRevision": "web-1", "updateRevision": "web-2",
    });
    let fresh = json!({ "observedGeneration": 4, "readyReplicas": 0 });
    let spec = json!({ "replicas": 3 });

    assert_eq!(
        issue(WorkloadKind::StatefulSet, spec.clone(), steady).as_deref(),
        Some("StatefulSet apps/web: 1 of 3 replicas ready")
    );
    assert_eq!(
        issue(WorkloadKind::StatefulSet, spec.clone(), updating),
        None
    );
    assert_eq!(issue(WorkloadKind::StatefulSet, spec, fresh), None);
}

#[test]
fn daemon_set_is_unhealthy_only_when_pods_are_unavailable_after_the_update() {
    let unavailable = json!({
        "observedGeneration": 4, "desiredNumberScheduled": 5,
        "updatedNumberScheduled": 5, "numberUnavailable": 2,
    });
    let updating = json!({
        "observedGeneration": 4, "desiredNumberScheduled": 5,
        "updatedNumberScheduled": 2, "numberUnavailable": 2,
    });
    let healthy = json!({
        "observedGeneration": 4, "desiredNumberScheduled": 5,
        "updatedNumberScheduled": 5, "numberUnavailable": 0,
    });

    assert_eq!(
        issue(WorkloadKind::DaemonSet, json!({}), unavailable).as_deref(),
        Some("DaemonSet apps/web: 3 of 5 pods available")
    );
    assert_eq!(issue(WorkloadKind::DaemonSet, json!({}), updating), None);
    assert_eq!(issue(WorkloadKind::DaemonSet, json!({}), healthy), None);
}

#[test]
fn a_stale_observed_generation_is_never_reported() {
    let status = json!({ "observedGeneration": 3, "updatedReplicas": 3, "readyReplicas": 0 });

    assert_eq!(
        issue(WorkloadKind::Deployment, json!({ "replicas": 3 }), status),
        None
    );
}

#[test]
fn finding_is_attributed_to_the_owner_named_by_the_flux_labels() {
    let status = json!({ "observedGeneration": 4, "updatedReplicas": 1 });
    let item = workload(json!({}), status);

    let found = finding(WorkloadKind::Deployment, Kind::Kustomization, item.clone())
        .unwrap()
        .unwrap();
    assert_eq!(
        found.owner,
        Target::new(Kind::Kustomization, "ci", "app").unwrap()
    );
    assert!(finding(WorkloadKind::Deployment, Kind::HelmRelease, item)
        .unwrap()
        .is_none());
}

#[test]
fn unhealthy_workloads_are_reported_only_after_they_stay_unhealthy() {
    let start = Instant::now();
    let mut tracker = UnhealthyWorkloads::default();

    assert!(tracker
        .lasting(vec![warming_daemon_set(0)], &[], start)
        .is_empty());
    assert!(tracker
        .lasting(
            vec![warming_daemon_set(0)],
            &[],
            start + Duration::from_secs(90)
        )
        .is_empty());
    assert_eq!(
        messages(tracker.lasting(vec![warming_daemon_set(0)], &[], start + UNSETTLED_GRACE)),
        ["DaemonSet apps/web: 2 of 3 pods available"]
    );
}

#[test]
fn min_ready_seconds_extend_the_wait_before_reporting() {
    let start = Instant::now();
    let mut tracker = UnhealthyWorkloads::default();
    tracker.lasting(vec![warming_daemon_set(300)], &[], start);

    assert!(tracker
        .lasting(vec![warming_daemon_set(300)], &[], start + UNSETTLED_GRACE)
        .is_empty());
    assert_eq!(
        tracker
            .lasting(
                vec![warming_daemon_set(300)],
                &[],
                start + UNSETTLED_GRACE + Duration::from_secs(300),
            )
            .len(),
        1
    );
}

#[test]
fn recovering_restarts_the_wait() {
    let start = Instant::now();
    let mut tracker = UnhealthyWorkloads::default();
    tracker.lasting(vec![warming_daemon_set(0)], &[], start);
    tracker.lasting(Vec::new(), &[], start + Duration::from_secs(60));

    assert!(tracker
        .lasting(vec![warming_daemon_set(0)], &[], start + UNSETTLED_GRACE)
        .is_empty());
}

#[test]
fn a_failed_list_keeps_the_wait_of_the_workloads_it_would_have_returned() {
    let start = Instant::now();
    let mut tracker = UnhealthyWorkloads::default();
    tracker.lasting(vec![warming_daemon_set(0)], &[], start);

    tracker.lasting(
        Vec::new(),
        &[(Kind::Kustomization, WorkloadKind::DaemonSet)],
        start + Duration::from_secs(60),
    );

    assert_eq!(
        messages(tracker.lasting(vec![warming_daemon_set(0)], &[], start + UNSETTLED_GRACE)),
        ["DaemonSet apps/web: 2 of 3 pods available"]
    );
}

#[test]
fn a_failure_of_another_list_does_not_keep_a_recovered_workload() {
    let start = Instant::now();
    let mut tracker = UnhealthyWorkloads::default();
    tracker.lasting(vec![warming_daemon_set(0)], &[], start);

    tracker.lasting(
        Vec::new(),
        &[(Kind::Kustomization, WorkloadKind::Deployment)],
        start + Duration::from_secs(60),
    );

    assert!(tracker
        .lasting(vec![warming_daemon_set(0)], &[], start + UNSETTLED_GRACE)
        .is_empty());
}

#[test]
fn a_missed_progress_deadline_is_reported_at_once() {
    let deadline = found(
        WorkloadKind::Deployment,
        json!({ "replicas": 3 }),
        json!({
            "observedGeneration": 4, "updatedReplicas": 1, "readyReplicas": 3,
            "conditions": [progressing("False", "ProgressDeadlineExceeded")],
        }),
    )
    .unwrap();

    assert_eq!(
        UnhealthyWorkloads::default()
            .lasting(vec![deadline], &[], Instant::now())
            .len(),
        1
    );
}
