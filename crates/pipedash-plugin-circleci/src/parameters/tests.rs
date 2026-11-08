use serde_json::json;

use super::*;

#[test]
fn pipeline_parameters_keep_their_order_types_and_defaults() {
    let source = "version: 2.1\nparameters:\n  deploy:\n    type: boolean\n    default: false\n    description: Deploy after tests\n  target:\n    type: enum\n    enum: [staging, production]\n    default: staging\n  retries:\n    type: integer\n    default: 2\n  release:\n    type: string\n  docker:\n    type: executor\n    default: node\njobs: {}\n";

    let parameters = pipeline_parameters(source).unwrap();

    let summary: Vec<_> = parameters
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
                "parameters.deploy",
                false,
                json!({"type": "boolean", "default": false})
            ),
            (
                "parameters.target",
                false,
                json!({"type": "choice", "options": ["staging", "production"], "default": "staging"})
            ),
            (
                "parameters.retries",
                false,
                json!({"type": "number", "default": 2.0})
            ),
            (
                "parameters.release",
                true,
                json!({"type": "string", "default": null})
            ),
        ]
    );
    assert_eq!(
        parameters[0].description.as_deref(),
        Some("Deploy after tests")
    );
    assert!(pipeline_parameters("version: 2.1\njobs: {}\n")
        .unwrap()
        .is_empty());
    assert!(pipeline_parameters(": : :").is_err());
}

#[test]
fn trigger_inputs_split_branch_definition_and_typed_parameters() {
    let inputs = trigger_inputs(Some(json!({
        "branch": " release/next ",
        "pipeline": "def-1",
        "parameters.deploy": true,
        "parameters.retries": 3.0,
        "parameters.ratio": 0.5,
        "parameters.release": "",
        "parameters.target": "production",
        "unrelated": "dropped"
    })));

    assert_eq!(inputs.branch.as_deref(), Some("release/next"));
    assert_eq!(inputs.definition.as_deref(), Some("def-1"));
    assert_eq!(
        Value::Object(inputs.parameters),
        json!({"deploy": true, "retries": 3, "ratio": 0.5, "release": "", "target": "production"})
    );
    let empty = trigger_inputs(None);
    assert!(empty.branch.is_none() && empty.parameters.is_empty());
}
