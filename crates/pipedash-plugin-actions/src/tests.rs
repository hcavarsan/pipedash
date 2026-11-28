use super::*;

#[test]
fn dispatch_inputs_are_read_from_the_workflow_file() {
    let source = "name: Release\non:\n  push:\n  workflow_dispatch:\n    inputs:\n      version:\n        description: Release version\n        required: true\n      dry_run:\n        type: boolean\n        default: true\n      level:\n        type: choice\n        options: [debug, info]\n        default: info\n      retries:\n        type: number\n        default: 3\njobs: {}\n";
    let params = dispatch_inputs(source).unwrap();
    let summary: Vec<_> = params
        .iter()
        .map(|p| {
            (
                p.name.as_str(),
                p.required,
                serde_json::to_value(&p.param_type).unwrap(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (
                "inputs.version",
                true,
                serde_json::json!({"type": "string", "default": null})
            ),
            (
                "inputs.dry_run",
                false,
                serde_json::json!({"type": "boolean", "default": true})
            ),
            (
                "inputs.level",
                false,
                serde_json::json!({"type": "choice", "options": ["debug", "info"], "default": "info"})
            ),
            (
                "inputs.retries",
                false,
                serde_json::json!({"type": "number", "default": 3.0})
            ),
        ]
    );
}

#[test]
fn dispatch_body_keeps_a_workflow_input_named_ref_separate_from_the_branch() {
    let body = dispatch_body(Some(serde_json::json!({
        "ref": "release",
        "inputs.ref": "abc123",
        "inputs.greeting": "hi"
    })));
    assert_eq!(
        body,
        serde_json::json!({"ref": "release", "inputs": {"ref": "abc123", "greeting": "hi"}})
    );
    assert_eq!(dispatch_body(None), serde_json::json!({"ref": "main"}));
}

#[test]
fn run_display_labels_kept_dispatch_inputs_by_their_workflow_name() {
    let inputs = serde_json::json!({
        "ref": "release",
        "inputs.ref": "abc123",
        "inputs.environment": "production"
    });
    let metadata = std::collections::HashMap::new();
    let display = run_display(&RunFacts {
        run_number: 8,
        status: pipedash_plugin_api::PipelineStatus::Running,
        branch: Some("release"),
        actor: None,
        inputs: Some(&inputs),
        metadata: &metadata,
    });

    let fields: Vec<_> = display
        .inputs
        .iter()
        .map(|field| (field.label.as_str(), field.value.as_str()))
        .collect();
    assert_eq!(fields, [("environment", "production"), ("ref", "abc123")]);
}

#[test]
fn workflows_without_dispatch_inputs_have_no_parameters() {
    assert!(dispatch_inputs("on: push\njobs: {}\n").unwrap().is_empty());
    assert!(dispatch_inputs("on: [push, workflow_dispatch]\n")
        .unwrap()
        .is_empty());
    assert!(dispatch_inputs("on:\n  workflow_dispatch:\n")
        .unwrap()
        .is_empty());
}
