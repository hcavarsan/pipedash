use std::collections::HashMap;

use pipedash_plugin_api::{
    ActionIcon,
    PipelineFacts,
    PipelineStatus,
    Plugin,
    RerunStrategy,
    RunFacts,
};
use serde_json::{
    json,
    Value,
};

use crate::FluxcdPlugin;

fn metadata(entries: &[(&str, Value)]) -> HashMap<String, Value> {
    entries
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect()
}

fn pipeline(metadata: &HashMap<String, Value>) -> PipelineFacts<'_> {
    PipelineFacts {
        id: "apps__web",
        status: PipelineStatus::Success,
        branch: None,
        workflow_file: None,
        resource_state: None,
        metadata,
    }
}

fn run<'a>(status: PipelineStatus, metadata: &'a HashMap<String, Value>) -> RunFacts<'a> {
    RunFacts {
        run_number: 4,
        status,
        branch: Some("main"),
        actor: Some("alice"),
        inputs: None,
        metadata,
    }
}

#[test]
fn pipeline_actions_should_reconcile_the_current_state_with_a_refresh_icon() {
    let empty = HashMap::new();
    let state = FluxcdPlugin::new().pipeline_actions(&pipeline(&empty));

    assert_eq!(state.rerun_strategy, RerunStrategy::ReconcileCurrent);
    assert_eq!(state.trigger_icon, ActionIcon::Refresh);
    assert!(state.trigger.allowed && state.trigger.reason.is_none());
    assert!(!state.cancel.allowed);
}

#[test]
fn pipeline_actions_should_block_a_missing_resource() {
    let missing = metadata(&[("missing", json!(true))]);
    let state = FluxcdPlugin::new().pipeline_actions(&pipeline(&missing));

    assert!(!state.trigger.allowed);
    assert_eq!(
        state.trigger.reason.as_deref(),
        Some("This resource was not found. Update the selection or recreate it in Flux.")
    );
}

#[test]
fn pipeline_actions_should_block_a_suspended_resource() {
    let suspended = metadata(&[("suspended", json!(true))]);
    let state = FluxcdPlugin::new().pipeline_actions(&pipeline(&suspended));

    assert!(!state.trigger.allowed);
    assert_eq!(
        state.trigger.reason.as_deref(),
        Some("This resource is suspended. Resume it in Flux before requesting reconciliation.")
    );
}

#[test]
fn pipeline_actions_should_report_a_missing_resource_before_a_suspended_one() {
    let both = metadata(&[("missing", json!(true)), ("suspended", json!(true))]);
    let state = FluxcdPlugin::new().pipeline_actions(&pipeline(&both));

    assert!(state
        .trigger
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("not found")));
}

#[test]
fn run_actions_should_block_rerun_for_missing_and_suspended_resources_only() {
    let plugin = FluxcdPlugin::new();
    let empty = HashMap::new();
    let ready = plugin.run_actions(&pipeline(&empty), &run(PipelineStatus::Success, &empty));
    assert!(ready.rerun.allowed);

    let suspended = metadata(&[("suspended", json!(true))]);
    let blocked = plugin.run_actions(&pipeline(&suspended), &run(PipelineStatus::Success, &empty));
    assert!(!blocked.rerun.allowed);
    assert!(blocked
        .rerun
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("suspended")));

    let missing = metadata(&[("missing", json!(true))]);
    let gone = plugin.run_actions(&pipeline(&missing), &run(PipelineStatus::Failed, &empty));
    assert!(!gone.rerun.allowed);
}

#[test]
fn run_display_should_title_the_row_with_the_run_label_or_the_default() {
    let plugin = FluxcdPlugin::new();
    let empty = HashMap::new();
    let labelled = metadata(&[("run_label", json!("Reconciliation abc1234"))]);

    let display = plugin.run_display(&pipeline(&empty), &run(PipelineStatus::Success, &labelled));
    let bare = plugin.run_display(&pipeline(&empty), &run(PipelineStatus::Success, &empty));

    assert_eq!(display.title, "Reconciliation abc1234");
    assert!(display.context.is_empty());
    assert_eq!(display.started_label, "Recorded");
    assert_eq!(bare.title, "Reconciliation");
}

#[test]
fn configured_repositories_should_hide_the_namespace_selection() {
    let config = HashMap::from([("selected_items".to_string(), "apps/web".to_string())]);
    assert!(FluxcdPlugin::new()
        .configured_repositories(&config)
        .is_empty());
}
