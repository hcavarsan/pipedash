use std::collections::HashMap;

use pipedash_plugin_api::PipelineStatus;
use serde_json::{
    json,
    Value,
};

use super::can_archive_run;

fn metadata(entries: &[(&str, Value)]) -> HashMap<String, Value> {
    entries
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect()
}

fn pipeline() -> HashMap<String, Value> {
    metadata(&[("uid", json!("uid-1"))])
}

fn run() -> HashMap<String, Value> {
    metadata(&[
        ("uid", json!("uid-1")),
        ("record_source", json!("status.history")),
    ])
}

#[test]
fn can_archive_run_should_accept_a_finished_history_entry_of_the_same_resource() {
    assert!(can_archive_run(
        &pipeline(),
        &run(),
        PipelineStatus::Success
    ));
    assert!(can_archive_run(&pipeline(), &run(), PipelineStatus::Failed));
}

#[test]
fn can_archive_run_should_reject_a_pipeline_without_a_uid() {
    assert!(!can_archive_run(
        &HashMap::new(),
        &run(),
        PipelineStatus::Success
    ));
    assert!(!can_archive_run(
        &metadata(&[("uid", json!(7))]),
        &run(),
        PipelineStatus::Success
    ));
}

#[test]
fn can_archive_run_should_reject_a_run_of_another_uid() {
    let other = metadata(&[
        ("uid", json!("uid-2")),
        ("record_source", json!("status.history")),
    ]);
    assert!(!can_archive_run(
        &pipeline(),
        &other,
        PipelineStatus::Success
    ));

    let unset = metadata(&[("record_source", json!("status.history"))]);
    assert!(!can_archive_run(
        &pipeline(),
        &unset,
        PipelineStatus::Success
    ));
}

#[test]
fn can_archive_run_should_reject_entries_that_are_not_status_history() {
    let snapshot = metadata(&[
        ("uid", json!("uid-1")),
        ("record_source", json!("current_snapshot")),
    ]);
    assert!(!can_archive_run(
        &pipeline(),
        &snapshot,
        PipelineStatus::Success
    ));

    let unset = metadata(&[("uid", json!("uid-1"))]);
    assert!(!can_archive_run(
        &pipeline(),
        &unset,
        PipelineStatus::Success
    ));
}

#[test]
fn can_archive_run_should_reject_running_and_pending_entries() {
    assert!(!can_archive_run(
        &pipeline(),
        &run(),
        PipelineStatus::Running
    ));
    assert!(!can_archive_run(
        &pipeline(),
        &run(),
        PipelineStatus::Pending
    ));
}
