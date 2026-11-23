use super::*;
use crate::config::Kind;
use crate::types::CronWorkflow;

fn workflow(phase: &str, shutdown: Option<&str>) -> Workflow {
    let mut value = json!({
        "metadata": {
            "name": "build-x7k2p",
            "namespace": "ci",
            "uid": "0000002a-1111-2222-3333-444444444444",
            "creationTimestamp": "2026-11-20T10:00:00Z",
            "labels": {
                "workflows.argoproj.io/workflow-template": "build",
                "workflows.argoproj.io/creator-preferred-username": "maria"
            }
        },
        "status": {
            "phase": phase,
            "startedAt": "2026-11-20T10:00:05Z",
            "finishedAt": "2026-11-20T10:02:05Z",
            "progress": "3/3",
            "message": "child failed"
        }
    });
    if let Some(strategy) = shutdown {
        value["spec"] = json!({ "shutdown": strategy });
    }
    serde_json::from_value(value).unwrap()
}

#[test]
fn every_workflow_phase_maps_to_a_status() {
    let cases = [
        ("", None, PipelineStatus::Pending),
        ("Pending", None, PipelineStatus::Pending),
        ("Running", None, PipelineStatus::Running),
        ("Succeeded", None, PipelineStatus::Success),
        ("Failed", None, PipelineStatus::Failed),
        ("Error", None, PipelineStatus::Failed),
        ("Failed", Some("Stop"), PipelineStatus::Cancelled),
        ("Failed", Some("Terminate"), PipelineStatus::Cancelled),
        ("Error", Some("Terminate"), PipelineStatus::Cancelled),
        ("Running", Some("Stop"), PipelineStatus::Running),
    ];
    for (phase, shutdown, expected) in cases {
        assert_eq!(
            map_status(&workflow(phase, shutdown)),
            expected,
            "{phase} {shutdown:?}"
        );
    }
}

#[test]
fn a_workflow_becomes_a_run_with_name_times_and_creator() {
    let pipeline = PipelineRef::new("ci", Kind::Template, "build");
    let run = map_run(&pipeline, &workflow("Failed", None));

    assert_eq!(run.pipeline_id, "ci__template__build");
    assert_eq!(run.run_number, 1_795_168_800_000_042);
    assert_eq!(run.status, PipelineStatus::Failed);
    assert_eq!(run.started_at.to_rfc3339(), "2026-11-20T10:00:05+00:00");
    assert_eq!(run.duration_seconds, Some(120));
    assert_eq!(run.actor.as_deref(), Some("maria"));
    assert_eq!(run.metadata["workflow"], "build-x7k2p");
    assert_eq!(run.metadata["progress"], "3/3");
    assert_eq!(run.metadata["message"], "child failed");
}

#[test]
fn a_workflow_still_pending_has_no_duration_and_starts_at_creation() {
    let pending: Workflow = serde_json::from_value(json!({
        "metadata": { "name": "nightly-1", "creationTimestamp": "2026-11-20T10:00:00Z" }
    }))
    .unwrap();
    let run = map_run(&PipelineRef::new("ci", Kind::Cron, "nightly"), &pending);

    assert_eq!(run.status, PipelineStatus::Pending);
    assert_eq!(run.started_at.to_rfc3339(), "2026-11-20T10:00:00+00:00");
    assert_eq!(run.duration_seconds, None);
    assert!(!run.metadata.contains_key("progress"));
}

#[test]
fn newest_workflows_come_first() {
    let mut workflows: Vec<Workflow> = ["2026-11-19T10:00:00Z", "2026-11-21T10:00:00Z"]
        .iter()
        .map(|created| {
            serde_json::from_value(json!({ "metadata": { "creationTimestamp": created } })).unwrap()
        })
        .collect();
    newest_first(&mut workflows);
    assert_eq!(
        workflows[0].metadata.creation_timestamp.as_deref(),
        Some("2026-11-21T10:00:00Z")
    );
}

#[test]
fn a_suspended_cron_workflow_shows_its_schedules_and_state() {
    let cron: CronWorkflow = serde_json::from_value(json!({
        "metadata": { "name": "nightly", "namespace": "ci" },
        "spec": { "schedules": ["0 2 * * *", "0 14 * * *"], "suspend": true }
    }))
    .unwrap();
    let pipeline = PipelineRef::new("ci", Kind::Cron, "nightly");
    let mapped = map_pipeline(
        &pipeline,
        Some(&cron.spec),
        Some(&workflow("Succeeded", None)),
    );

    assert_eq!(mapped.id, "ci__cron__nightly");
    assert_eq!(mapped.status, PipelineStatus::Success);
    assert_eq!(mapped.metadata["kind"], "Cron workflow");
    assert_eq!(mapped.metadata["schedule"], "0 2 * * *, 0 14 * * *");
    assert_eq!(mapped.resource_state.unwrap().label, "Suspended");
}

#[test]
fn a_template_without_runs_is_pending() {
    let pipeline = PipelineRef::new("ci", Kind::Template, "build");
    let mapped = map_pipeline(&pipeline, None, None);

    assert_eq!(mapped.status, PipelineStatus::Pending);
    assert_eq!(mapped.last_run, None);
    assert!(mapped.resource_state.is_none());
    assert!(!mapped.metadata.contains_key("schedule"));
}
