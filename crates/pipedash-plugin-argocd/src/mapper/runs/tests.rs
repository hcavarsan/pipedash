use pipedash_plugin_api::{
    CommitSha,
    Observed,
    PipelineStatus,
    RunEvidence,
};
use serde_json::{
    json,
    Value,
};

use super::{
    map_history_to_run,
    map_operation_to_run,
};
use crate::types::Application;

fn app(history: Value) -> Application {
    serde_json::from_value(json!({
        "metadata": {"name": "web", "namespace": "argocd"},
        "spec": {
            "source": {"repoURL": "https://github.com/acme/web", "path": "deploy"},
            "destination": {"server": "https://prod.example.com:6443", "namespace": "web"},
            "project": "default"
        },
        "status": {
            "sync": {"status": "OutOfSync"},
            "health": {"status": "Degraded"},
            "history": [history],
            "operationState": {
                "phase": "Failed",
                "startedAt": "2026-10-20T11:00:00Z",
                "finishedAt": "2026-10-20T11:00:42Z"
            }
        }
    }))
    .unwrap()
}

#[test]
fn history_run_has_absent_outcome_and_no_app_health_status() {
    let app = app(json!({
        "id": 3,
        "revision": "abc",
        "deployedAt": "2026-10-20T10:00:30Z",
        "deployStartedAt": "2026-10-20T10:00:00Z"
    }));
    let history = &app.status.history.as_ref().unwrap()[0];

    let run = map_history_to_run(history, &app, "");

    assert_eq!(
        run.evidence,
        RunEvidence {
            outcome: Observed::Absent,
            timing: Observed::Recorded,
            attempt: false,
        }
    );
    assert!(!run.metadata.contains_key("app_health_status"));
    assert!(!run.metadata.contains_key("app_sync_status"));
    assert_eq!(run.started_at, history.deploy_started_at.unwrap());
    assert_eq!(run.concluded_at, Some(history.deployed_at));
    assert_eq!(run.duration_seconds, Some(30));
}

#[test]
fn history_run_without_a_start_time_has_no_timing() {
    let app = app(json!({
        "id": 3,
        "revision": "abc",
        "deployedAt": "2026-10-20T10:00:30Z"
    }));
    let history = &app.status.history.as_ref().unwrap()[0];

    let run = map_history_to_run(history, &app, "");

    assert_eq!(
        run.evidence,
        RunEvidence {
            outcome: Observed::Absent,
            timing: Observed::Absent,
            attempt: false,
        }
    );
    assert_eq!(run.started_at, history.deployed_at);
    assert_eq!(run.concluded_at, Some(history.deployed_at));
    assert_eq!(run.duration_seconds, None);
}

#[test]
fn operation_run_keeps_recorded_evidence_and_the_app_status() {
    let app = app(json!({"id": 3, "deployedAt": "2026-10-20T10:00:30Z"}));

    let run = map_operation_to_run(&app, "").unwrap();

    assert_eq!(run.status, PipelineStatus::Failed);
    assert_eq!(run.evidence, RunEvidence::default());
    assert_eq!(run.duration_seconds, Some(42));
    assert_eq!(
        run.metadata.get("app_health_status"),
        Some(&json!("Degraded"))
    );
    assert_eq!(
        run.metadata.get("app_sync_status"),
        Some(&json!("OutOfSync"))
    );
}

#[test]
fn history_run_correlation_is_bound_to_its_revision() {
    let app = app(json!({
        "id": 3,
        "deployedAt": "2026-10-20T10:00:30Z",
        "source": {"repoURL": "https://github.com/acme/web", "path": "deploy"},
        "revision": "a".repeat(40)
    }));
    let history = &app.status.history.as_ref().unwrap()[0];

    let run = map_history_to_run(history, &app, "");

    assert_eq!(run.correlation.sources.len(), 1);
    assert_eq!(
        run.correlation.sources[0].commit,
        CommitSha::parse(&"a".repeat(40))
    );
}
