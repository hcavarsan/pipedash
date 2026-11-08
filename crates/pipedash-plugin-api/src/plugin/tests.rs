use serde_json::{
    json,
    Map,
    Value,
};

use super::*;
use crate::actions::{
    ActionIcon,
    RerunStrategy,
};
use crate::metadata::PluginCapabilities;
use crate::presentation::DisplayField;
use crate::testing::Stub;

fn plugin(capabilities: PluginCapabilities) -> Stub {
    Stub::new(capabilities)
}

fn pipeline(metadata: &HashMap<String, Value>) -> PipelineFacts<'_> {
    PipelineFacts {
        id: "stub__1__build",
        status: PipelineStatus::Success,
        branch: Some("main"),
        workflow_file: None,
        resource_state: None,
        metadata,
    }
}

fn run<'a>(
    status: PipelineStatus, metadata: &'a HashMap<String, Value>, inputs: Option<&'a Value>,
) -> RunFacts<'a> {
    RunFacts {
        run_number: 7,
        status,
        branch: Some("feature"),
        actor: Some("ada"),
        commit_message: None,
        inputs,
        metadata,
    }
}

fn parameter(name: &str, param_type: WorkflowParameterType) -> WorkflowParameter {
    WorkflowParameter {
        name: name.to_string(),
        label: None,
        description: None,
        param_type,
        required: false,
    }
}

#[test]
fn force_cancellation_defaults_to_unsupported_for_existing_capabilities() {
    let capabilities = PluginCapabilities::default();
    assert!(!capabilities.force_cancel);

    let mut serialized = serde_json::to_value(capabilities).unwrap();
    serialized.as_object_mut().unwrap().remove("force_cancel");
    let restored: PluginCapabilities = serde_json::from_value(serialized).unwrap();
    assert!(!restored.force_cancel);
}

#[test]
fn new_capabilities_default_to_false_for_stored_metadata() {
    let mut serialized = serde_json::to_value(PluginCapabilities::standard()).unwrap();
    let object = serialized.as_object_mut().unwrap();
    object.remove("uses_http_proxy");
    object.remove("async_trigger");
    let restored: PluginCapabilities = serde_json::from_value(serialized).unwrap();
    assert!(!restored.uses_http_proxy && !restored.async_trigger);
}

#[test]
fn default_pipeline_actions_honor_capabilities() {
    let metadata = HashMap::new();
    let facts = pipeline(&metadata);

    let none = plugin(PluginCapabilities::default()).pipeline_actions(&facts);
    assert!(!none.trigger.allowed && !none.cancel.allowed && !none.force_cancel.allowed);
    assert!(!none.enable.allowed);

    let some = plugin(PluginCapabilities {
        trigger: true,
        force_cancel: true,
        ..PluginCapabilities::default()
    })
    .pipeline_actions(&facts);
    assert!(some.trigger.allowed && some.force_cancel.allowed);
    assert!(!some.cancel.allowed);
    assert_eq!(some.rerun_strategy, RerunStrategy::ReplayInputs);
    assert_eq!(some.trigger_icon, ActionIcon::Play);
    assert!(some.target.is_none());

    let enabled = HashMap::from([("can_enable".to_string(), json!(true))]);
    let state = plugin(PluginCapabilities::default()).pipeline_actions(&pipeline(&enabled));
    assert!(state.enable.allowed);
}

#[test]
fn default_run_actions_follow_status_and_capabilities() {
    let plugin = plugin(PluginCapabilities {
        trigger: true,
        cancel: true,
        force_cancel: true,
        ..PluginCapabilities::default()
    });
    let empty = HashMap::new();
    let facts = pipeline(&empty);

    let running = plugin.run_actions(&facts, &run(PipelineStatus::Running, &empty, None));
    assert!(running.cancel.allowed && running.force_cancel.allowed && running.watch);
    assert!(!running.rerun.allowed && !running.review.allowed);

    let review = HashMap::from([("awaiting_review".to_string(), json!(true))]);
    let waiting = plugin.run_actions(&facts, &run(PipelineStatus::Pending, &review, None));
    assert!(!waiting.cancel.allowed && waiting.review.allowed);

    let failed_jobs = HashMap::from([("can_rerun_failed_jobs".to_string(), json!(true))]);
    let done = plugin.run_actions(&facts, &run(PipelineStatus::Failed, &failed_jobs, None));
    assert!(done.rerun.allowed && done.rerun_failed.allowed);
    assert!(!done.cancel.allowed && !done.watch);
}

#[test]
fn default_run_display_lists_branch_and_actor() {
    let empty = HashMap::new();
    let facts = pipeline(&empty);
    let plugin = plugin(PluginCapabilities::default());

    let display = plugin.run_display(&facts, &run(PipelineStatus::Success, &empty, None));
    assert_eq!(display.title, "#7");
    assert_eq!(display.message, None);
    assert_eq!(display.started_label, "Started");
    assert_eq!(
        display.context,
        vec![
            DisplayField {
                label: "Branch".to_string(),
                value: "feature".to_string()
            },
            DisplayField {
                label: "Actor".to_string(),
                value: "ada".to_string()
            },
        ]
    );

    let mut sparse = run(PipelineStatus::Success, &empty, None);
    sparse.branch = Some("");
    sparse.actor = None;
    assert!(plugin.run_display(&facts, &sparse).context.is_empty());
}

#[test]
fn default_trigger_values_replay_previous_inputs_and_inject_the_branch() {
    let empty = HashMap::new();
    let facts = pipeline(&empty);
    let plugin = plugin(PluginCapabilities::default());
    let parameters = [
        parameter("ref", WorkflowParameterType::String { default: None }),
        parameter(
            "target",
            WorkflowParameterType::Choice {
                options: vec!["dev".to_string(), "prod".to_string()],
                default: Some("dev".to_string()),
            },
        ),
        parameter("dry_run", WorkflowParameterType::Boolean { default: false }),
        parameter(
            "retries",
            WorkflowParameterType::Number { default: Some(3.0) },
        ),
        parameter(
            "note",
            WorkflowParameterType::String {
                default: Some("  ".to_string()),
            },
        ),
    ];

    let defaults = plugin.trigger_values(&facts, None, &parameters);
    assert_eq!(
        Value::Object(defaults),
        json!({"target": "dev", "dry_run": false, "retries": 3.0})
    );

    let inputs = json!({"target": "prod", "unrelated": 1});
    let previous = run(PipelineStatus::Failed, &empty, Some(&inputs));
    let replayed = plugin.trigger_values(&facts, Some(&previous), &parameters);
    assert_eq!(
        Value::Object(replayed),
        json!({"ref": "feature", "target": "prod", "dry_run": false, "retries": 3.0})
    );

    let explicit = json!({"ref": "release"});
    let previous = run(PipelineStatus::Failed, &empty, Some(&explicit));
    let kept = plugin.trigger_values(&facts, Some(&previous), &parameters);
    assert_eq!(kept.get("ref"), Some(&json!("release")));
}

#[test]
fn default_trigger_values_without_parameters_are_empty() {
    let empty = HashMap::new();
    let facts = pipeline(&empty);
    let previous = run(PipelineStatus::Failed, &empty, None);
    let values: Map<String, Value> =
        plugin(PluginCapabilities::default()).trigger_values(&facts, Some(&previous), &[]);
    assert!(values.is_empty());
}

#[test]
fn default_configured_repositories_split_selected_items() {
    let plugin = plugin(PluginCapabilities::default());
    let config = HashMap::from([("selected_items".to_string(), " a/b, ,c/d ".to_string())]);
    assert_eq!(plugin.configured_repositories(&config), ["a/b", "c/d"]);
    assert!(plugin.configured_repositories(&HashMap::new()).is_empty());
    assert!(!plugin.can_archive_run(&HashMap::new(), &HashMap::new(), PipelineStatus::Success));
}
