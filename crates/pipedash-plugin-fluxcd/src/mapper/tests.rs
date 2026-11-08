use serde_json::json;

use super::*;

const CREATED: &str = "2026-10-20T10:00:00Z";

fn resource(kind: Kind, generation: i64, spec: Value, status: Value) -> FluxResource {
    FluxResource::from_value(
        kind,
        json!({
            "metadata": {
                "name": "app",
                "namespace": "ci",
                "uid": "uid-1",
                "generation": generation,
                "creationTimestamp": CREATED,
            },
            "spec": spec,
            "status": status,
        }),
    )
    .unwrap()
}

fn cond(type_: &str, status: &str, reason: &str, generation: i64, at: &str) -> Value {
    json!({
        "type": type_,
        "status": status,
        "reason": reason,
        "message": format!("{type_} {status} {reason}"),
        "observedGeneration": generation,
        "lastTransitionTime": at,
    })
}

fn kustomization(status: Value) -> FluxResource {
    resource(Kind::Kustomization, 2, json!({}), status)
}

fn at(text: &str) -> DateTime<Utc> {
    text.parse().unwrap()
}

#[test]
fn ready_condition_from_an_older_generation_is_pending_not_success() {
    let stale = kustomization(json!({
        "observedGeneration": 1,
        "conditions": [cond("Ready", "True", "ReconciliationSucceeded", 1, CREATED)],
    }));

    assert_eq!(current_status(&stale), PipelineStatus::Pending);
    assert_eq!(
        headline(&stale, PipelineStatus::Pending).0,
        "StatusOutdated"
    );
}

#[test]
fn condition_generation_wins_over_a_reset_status_generation() {
    let failing = kustomization(json!({
        "observedGeneration": -1,
        "conditions": [
            cond("Reconciling", "True", "ProgressingWithRetry", 2, CREATED),
            cond("Ready", "False", "ArtifactFailed", 2, CREATED),
        ],
    }));

    assert_eq!(current_status(&failing), PipelineStatus::Failed);
    assert_eq!(
        headline(&failing, PipelineStatus::Failed),
        (
            "ArtifactFailed".to_string(),
            "Ready False ArtifactFailed".to_string()
        )
    );
}

#[test]
fn conditions_map_to_ready_reconciling_stalled_and_pending() {
    let cases = [
        (
            vec![cond("Ready", "True", "ReconciliationSucceeded", 2, CREATED)],
            PipelineStatus::Success,
        ),
        (
            vec![cond("Ready", "False", "BuildFailed", 2, CREATED)],
            PipelineStatus::Failed,
        ),
        (
            vec![cond("Ready", "Unknown", "Progressing", 2, CREATED)],
            PipelineStatus::Running,
        ),
        (
            vec![
                cond("Reconciling", "True", "Progressing", 2, CREATED),
                cond("Ready", "True", "ReconciliationSucceeded", 2, CREATED),
            ],
            PipelineStatus::Running,
        ),
        (
            vec![
                cond("Stalled", "True", "RetriesExceeded", 2, CREATED),
                cond("Ready", "False", "InstallFailed", 2, CREATED),
            ],
            PipelineStatus::Failed,
        ),
        (vec![], PipelineStatus::Pending),
    ];

    for (conditions, expected) in cases {
        let resource = kustomization(json!({ "observedGeneration": 2, "conditions": conditions }));
        assert_eq!(current_status(&resource), expected, "{conditions:?}");
    }
}

#[test]
fn suspended_resources_are_skipped_even_when_ready() {
    let suspended = resource(
        Kind::HelmRelease,
        2,
        json!({ "suspend": true }),
        json!({ "conditions": [cond("Ready", "True", "InstallSucceeded", 2, CREATED)] }),
    );

    assert_eq!(current_status(&suspended), PipelineStatus::Skipped);
    assert_eq!(status_label(PipelineStatus::Skipped), "Suspended");
    assert_eq!(headline(&suspended, PipelineStatus::Skipped).0, "Suspended");
}

#[test]
fn go_durations_parse_fractions_and_compound_units() {
    let millis = |text| {
        parse_go_duration(text).map(|duration| (duration.as_secs_f64() * 1000.0).round() as i64)
    };

    assert_eq!(millis("2.5s"), Some(2500));
    assert_eq!(millis("1m30s"), Some(90_000));
    assert_eq!(millis("176.163666ms"), Some(176));
    assert_eq!(millis("45.236161127s"), Some(45_236));
    assert_eq!(millis("1h"), Some(3_600_000));
    assert_eq!(millis("fast"), None);
    assert_eq!(millis(""), None);
}

#[test]
fn revisions_split_into_reference_and_commit_digest() {
    assert_eq!(
        split_revision("6.7.1@sha1:6b7aab8a"),
        (Some("6.7.1"), Some("6b7aab8a"))
    );
    assert_eq!(split_revision("sha256:abcd"), (None, Some("abcd")));
    assert_eq!(split_revision("6.7.1"), (None, None));
}

fn kustomization_with_history(history: Value, conditions: Vec<Value>) -> FluxResource {
    kustomization(json!({
        "observedGeneration": 2,
        "history": history,
        "conditions": conditions,
    }))
}

fn entry(digest: &str, first: &str, last: &str, duration: &str, status: &str) -> Value {
    json!({
        "digest": digest,
        "firstReconciled": first,
        "lastReconciled": last,
        "lastReconciledDuration": duration,
        "lastReconciledStatus": status,
        "totalReconciliations": 3,
        "metadata": { "revision": "6.7.1@sha1:6b7aab8a" },
    })
}

fn number_of(runs: &[PipelineRun], digest: &str) -> Option<i64> {
    runs.iter()
        .find(|run| {
            run.metadata
                .get("digest")
                .is_some_and(|value| value == digest)
        })
        .map(|run| run.run_number)
}

fn digests(runs: &[PipelineRun]) -> Vec<&str> {
    runs.iter()
        .filter_map(|run| run.metadata.get("digest")?.as_str())
        .collect()
}

#[test]
fn kustomization_history_keeps_native_times_durations_and_labels() {
    let resource = kustomization_with_history(
        json!([
            entry(
                "sha256:b",
                "2026-10-21T10:00:00Z",
                "2026-10-21T12:00:45Z",
                "45s",
                "ReconciliationSucceeded"
            ),
            entry(
                "sha256:a",
                "2026-10-20T10:00:00Z",
                "2026-10-20T10:00:10Z",
                "10s",
                "HealthCheckFailed"
            ),
        ]),
        vec![cond(
            "Ready",
            "True",
            "ReconciliationSucceeded",
            2,
            "2026-10-21T12:00:45Z",
        )],
    );

    let runs = build_runs(&resource).unwrap();

    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].status, PipelineStatus::Success);
    assert_eq!(
        runs[0].run_number / 1_000_000,
        at("2026-10-21T10:00:00Z").timestamp()
    );
    assert_eq!(runs[0].started_at, at("2026-10-21T12:00:00Z"));
    assert_eq!(runs[0].concluded_at, Some(at("2026-10-21T12:00:45Z")));
    assert_eq!(runs[0].duration_seconds, Some(45));
    assert_eq!(runs[0].commit_sha.as_deref(), Some("6b7aab8a"));
    assert_eq!(runs[0].metadata["total_reconciliations"], json!(3));
    assert_eq!(
        runs[0].metadata["message"],
        json!("Ready True ReconciliationSucceeded")
    );
    assert_eq!(runs[1].status, PipelineStatus::Failed);
    assert_eq!(runs[1].metadata["status_label"], json!("Reconcile failed"));
    assert_eq!(runs[1].pipeline_id, "Kustomization__ci__app");
    assert!(runs[1].id.starts_with("Kustomization__ci__app__"));
}

#[test]
fn current_snapshot_is_added_only_when_history_does_not_cover_it() {
    let history = json!([entry(
        "sha256:a",
        "2026-10-20T10:00:00Z",
        "2026-10-20T10:00:10Z",
        "10s",
        "ReconciliationSucceeded"
    )]);
    let failing = kustomization_with_history(
        history.clone(),
        vec![cond(
            "Ready",
            "False",
            "ArtifactFailed",
            2,
            "2026-10-21T09:00:00Z",
        )],
    );

    let runs = build_runs(&failing).unwrap();

    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].metadata["record_source"], json!("current_snapshot"));
    assert_eq!(
        runs[0].metadata["time_source"],
        json!("condition.lastTransitionTime")
    );
    assert_eq!(runs[0].started_at, at("2026-10-21T09:00:00Z"));
    assert_eq!(runs[0].concluded_at, None);
    assert_eq!(runs[0].duration_seconds, None);
    assert_eq!(
        runs[0].run_number,
        runs[1].run_number / 1_000_000 * 1_000_000 + 999_999
    );
    assert_eq!(
        runs[0].metadata["message"],
        json!("Ready False ArtifactFailed")
    );
}

#[test]
fn snapshot_number_ignores_condition_message_and_transition_churn() {
    let first = kustomization(json!({
        "observedGeneration": 2,
        "conditions": [cond("Ready", "False", "ArtifactFailed", 2, "2026-10-21T09:00:00Z")],
    }));
    let retried = kustomization(json!({
        "observedGeneration": 2,
        "conditions": [{
            "type": "Ready", "status": "False", "reason": "ArtifactFailed",
            "message": "path not found: /tmp/other-dir", "observedGeneration": 2,
            "lastTransitionTime": "2026-10-21T09:01:00Z",
        }],
    }));

    let before = build_runs(&first).unwrap();
    let after = build_runs(&retried).unwrap();

    assert_eq!(before.len(), 1);
    assert_eq!(before[0].run_number, after[0].run_number);
    assert_eq!(before[0].id, after[0].id);
    assert_eq!(
        before[0].run_number,
        at(CREATED).timestamp() * 1_000_000 + 999_999
    );
}

#[test]
fn same_second_records_keep_their_identity_when_a_tied_record_is_evicted() {
    let a = entry(
        "sha256:a",
        "2026-10-20T10:00:00Z",
        "2026-10-20T10:00:10Z",
        "1s",
        "ReconciliationSucceeded",
    );
    let b = entry(
        "sha256:b",
        "2026-10-20T10:00:00Z",
        "2026-10-20T10:00:05Z",
        "1s",
        "HealthCheckFailed",
    );
    let ready = vec![cond("Ready", "True", "ReconciliationSucceeded", 2, CREATED)];

    let forward = build_runs(&kustomization_with_history(json!([a, b]), ready.clone())).unwrap();
    let reverse = build_runs(&kustomization_with_history(json!([b, a]), ready.clone())).unwrap();
    let evicted = build_runs(&kustomization_with_history(json!([b]), ready)).unwrap();

    assert_ne!(
        number_of(&forward, "sha256:a"),
        number_of(&forward, "sha256:b")
    );
    assert_eq!(
        number_of(&forward, "sha256:a"),
        number_of(&reverse, "sha256:a")
    );
    assert_eq!(
        number_of(&forward, "sha256:b"),
        number_of(&reverse, "sha256:b")
    );
    assert_eq!(
        number_of(&forward, "sha256:b"),
        number_of(&evicted, "sha256:b")
    );
    assert_eq!(number_of(&evicted, "sha256:a"), None);
}

#[test]
fn history_lists_the_latest_reconciliation_first_without_changing_identities() {
    let ready = vec![cond("Ready", "True", "ReconciliationSucceeded", 2, CREATED)];
    let reconciled = |last: &str| {
        entry(
            "sha256:a",
            "2026-10-10T10:00:00Z",
            last,
            "10s",
            "ReconciliationSucceeded",
        )
    };
    let middle = entry(
        "sha256:b",
        "2026-10-15T10:00:00Z",
        "2026-10-16T10:00:10Z",
        "10s",
        "ReconciliationSucceeded",
    );
    let build = |history: Vec<Value>| {
        build_runs(&kustomization_with_history(json!(history), ready.clone())).unwrap()
    };

    let before = build(vec![reconciled("2026-10-14T10:00:10Z"), middle.clone()]);
    let after = build(vec![middle, reconciled("2026-10-22T10:00:10Z")]);
    let evicted = build(vec![reconciled("2026-10-22T10:00:10Z")]);

    assert_eq!(digests(&before), ["sha256:b", "sha256:a"]);
    assert_eq!(digests(&after), ["sha256:a", "sha256:b"]);
    assert!(number_of(&after, "sha256:a") < number_of(&after, "sha256:b"));
    assert_eq!(
        number_of(&before, "sha256:a"),
        number_of(&after, "sha256:a")
    );
    assert_eq!(
        number_of(&before, "sha256:b"),
        number_of(&after, "sha256:b")
    );
    assert_eq!(
        number_of(&evicted, "sha256:a"),
        number_of(&after, "sha256:a")
    );
    assert_eq!(number_of(&evicted, "sha256:b"), None);
}

#[test]
fn records_observed_at_the_same_time_order_by_identity_in_any_input_order() {
    let ready = vec![cond("Ready", "True", "ReconciliationSucceeded", 2, CREATED)];
    let record = |digest: &str| {
        entry(
            digest,
            "2026-10-20T10:00:00Z",
            "2026-10-21T10:00:10Z",
            "10s",
            "ReconciliationSucceeded",
        )
    };
    let (a, b) = (record("sha256:a"), record("sha256:b"));

    let forward = build_runs(&kustomization_with_history(json!([a, b]), ready.clone())).unwrap();
    let reverse = build_runs(&kustomization_with_history(json!([b, a]), ready)).unwrap();

    assert_eq!(forward[0].started_at, forward[1].started_at);
    assert!(forward[0].run_number > forward[1].run_number);
    assert_eq!(digests(&forward), digests(&reverse));
}

#[test]
fn indistinguishable_history_records_are_an_error_not_an_alias() {
    let twin = entry(
        "sha256:a",
        "2026-10-20T10:00:00Z",
        "2026-10-20T10:00:10Z",
        "1s",
        "ReconciliationSucceeded",
    );
    let resource = kustomization_with_history(
        json!([twin, twin]),
        vec![cond("Ready", "True", "ReconciliationSucceeded", 2, CREATED)],
    );

    let error = build_runs(&resource).unwrap_err();

    assert!(
        matches!(&error, PluginError::SerializationError(message) if message.contains("Kustomization ci/app")),
        "{error:?}"
    );
}

#[test]
fn repeated_reconciliations_update_the_same_record_instead_of_creating_a_new_one() {
    let ready = vec![cond("Ready", "True", "ReconciliationSucceeded", 2, CREATED)];
    let earlier = entry(
        "sha256:a",
        "2026-10-20T10:00:00Z",
        "2026-10-20T10:00:10Z",
        "1s",
        "ReconciliationSucceeded",
    );
    let mut later = earlier.clone();
    later["lastReconciled"] = json!("2026-10-22T10:00:10Z");
    later["totalReconciliations"] = json!(9);

    let before = build_runs(&kustomization_with_history(json!([earlier]), ready.clone())).unwrap();
    let after = build_runs(&kustomization_with_history(json!([later]), ready)).unwrap();

    assert_eq!(before[0].id, after[0].id);
    assert_eq!(after[0].metadata["total_reconciliations"], json!(9));
    assert_ne!(before[0].concluded_at, after[0].concluded_at);
}

#[test]
fn helm_history_exposes_release_versions_and_deploy_times() {
    let resource = resource(
        Kind::HelmRelease,
        1,
        json!({ "chart": { "spec": { "chart": "podinfo", "version": "6.7.1", "sourceRef": { "kind": "HelmRepository", "name": "podinfo" } } } }),
        json!({
            "observedGeneration": 1,
            "lastAttemptedReleaseAction": "upgrade",
            "lastAttemptedReleaseActionDuration": "30.3s",
            "history": [
                { "version": 2, "action": "upgrade", "status": "failed", "chartName": "podinfo", "chartVersion": "6.7.2", "digest": "sha256:2", "firstDeployed": "2026-10-20T10:00:00Z", "lastDeployed": "2026-10-21T10:00:00Z" },
                { "version": 1, "action": "install", "status": "superseded", "chartName": "podinfo", "chartVersion": "6.7.1", "digest": "sha256:1", "firstDeployed": "2026-10-20T10:00:00Z", "lastDeployed": "2026-10-20T10:00:00Z" },
            ],
            "conditions": [cond("Ready", "False", "UpgradeFailed", 1, "2026-10-21T10:00:31Z")],
        }),
    );

    let runs = build_runs(&resource).unwrap();

    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].metadata["release_version"], json!(2));
    assert_eq!(runs[0].run_number % 1_000_000, 2);
    assert_eq!(runs[1].run_number % 1_000_000, 1);
    assert_eq!(
        runs[0].run_number / 1_000_000,
        at("2026-10-21T10:00:00Z").timestamp()
    );
    assert_eq!(runs[0].metadata["chart"], json!("podinfo@6.7.2"));
    assert_eq!(runs[0].status, PipelineStatus::Failed);
    assert_eq!(runs[0].started_at, at("2026-10-21T10:00:00Z"));
    assert_eq!(runs[0].duration_seconds, Some(30));
    assert_eq!(
        runs[0].metadata["message"],
        json!("Ready False UpgradeFailed")
    );
    assert_eq!(runs[1].status, PipelineStatus::Success);
    assert_eq!(runs[1].metadata["status_label"], json!("Superseded"));
    assert_eq!(runs[1].duration_seconds, None);
}

#[test]
fn pipeline_groups_by_namespace_and_source_with_kind_in_the_name() {
    let resource = resource(
        Kind::Kustomization,
        1,
        json!({ "path": "./kustomize", "suspend": true, "sourceRef": { "kind": "GitRepository", "name": "podinfo" } }),
        json!({}),
    );
    let pipeline = map_pipeline(&resource, &[]);

    assert_eq!(pipeline.id, "Kustomization__ci__app");
    assert_eq!(pipeline.name, "Kustomization/app");
    assert_eq!(pipeline.repository, "ci/podinfo");
    assert_eq!(pipeline.status, PipelineStatus::Skipped);
    assert_eq!(pipeline.metadata["suspended"], json!(true));
    assert_eq!(
        pipeline.resource_state,
        Some(ResourceState::new(
            "Suspended",
            Some("spec.suspend is true; Flux is not reconciling this resource")
        ))
    );
    assert!(!pipeline.metadata.contains_key("status_label"));
    assert!(!pipeline.metadata.contains_key("message"));
    assert_eq!(pipeline.metadata["reason"], json!("Suspended"));
    assert_eq!(
        pipeline.metadata["source_ref"],
        json!("GitRepository/ci/podinfo")
    );
    assert_eq!(
        pipeline.metadata["history_scope"],
        json!("bounded_reconciliation")
    );
}

fn ready(status: &str) -> FluxResource {
    kustomization(json!({
        "observedGeneration": 2,
        "conditions": [cond("Ready", status, "ReconciliationSucceeded", 2, CREATED)],
    }))
}

#[test]
fn workload_issues_degrade_only_a_ready_resource_and_summarize_extras() {
    let issues = vec![
        "Deployment apps/web: 1 of 3 replicas ready".to_string(),
        "StatefulSet apps/db: 0 of 1 replicas ready".to_string(),
        "DaemonSet apps/agent: 2 of 5 pods ready".to_string(),
    ];

    let degraded = map_pipeline(&ready("True"), &issues[..1]);
    assert_eq!(degraded.status, PipelineStatus::Degraded);
    assert_eq!(
        degraded.resource_state,
        Some(ResourceState::new(
            "Degraded",
            Some("Deployment apps/web: 1 of 3 replicas ready")
        ))
    );
    assert_eq!(
        degraded.metadata["reason"],
        json!("ReconciliationSucceeded")
    );

    let several = map_pipeline(&ready("True"), &issues);
    assert_eq!(
        several.resource_state.unwrap().message.as_deref(),
        Some("Deployment apps/web: 1 of 3 replicas ready (+2 more)")
    );

    for (status, expected, label) in [
        ("False", PipelineStatus::Failed, "Not ready"),
        ("Unknown", PipelineStatus::Running, "Reconciling"),
    ] {
        let pipeline = map_pipeline(&ready(status), &issues);
        assert_eq!(pipeline.status, expected);
        assert_eq!(pipeline.resource_state.unwrap().label, label);
    }

    let healthy = map_pipeline(&ready("True"), &[]);
    assert_eq!(healthy.status, PipelineStatus::Success);
    assert_eq!(
        healthy.resource_state,
        Some(ResourceState::new(
            "Ready",
            Some("Ready True ReconciliationSucceeded")
        ))
    );
}

#[test]
fn snapshot_run_keeps_label_and_message_in_run_metadata() {
    let runs = build_runs(&ready("False")).unwrap();

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].metadata["status_label"], json!("Not ready"));
    assert_eq!(
        runs[0].metadata["message"],
        json!("Ready False ReconciliationSucceeded")
    );
    assert_eq!(runs[0].metadata["reason"], json!("ReconciliationSucceeded"));
}

#[test]
fn run_numbers_stay_within_javascript_safe_integers() {
    let resource = kustomization_with_history(
        json!([entry(
            "sha256:a",
            "9999-12-31T23:59:59Z",
            "9999-12-31T23:59:59Z",
            "1s",
            "ReconciliationSucceeded"
        )]),
        vec![cond("Ready", "False", "BuildFailed", 2, CREATED)],
    );

    let runs = build_runs(&resource).unwrap();

    assert!(runs.iter().all(|run| run.run_number < (1 << 53)));
}
