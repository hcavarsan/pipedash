use pipedash_plugin_api::{
    DisplayField,
    TriggerResult,
    WorkflowParameter,
};
use serde_json::{
    json,
    Map,
    Value,
};

use super::validate_params;
use crate::application::services::presenter::READ_ONLY_PROVIDER;
use crate::application::test_support::plugins::{
    registry,
    STUB,
};
use crate::application::test_support::{
    parameter,
    pipeline,
    stub_run,
    Harness,
    StubProvider,
};
use crate::domain::{
    DomainError,
    PipelineStatus,
    TriggerParams,
};

fn required(name: &str, label: Option<&str>) -> WorkflowParameter {
    serde_json::from_value(json!({
        "name": name, "label": label, "type": "string", "required": true
    }))
    .unwrap()
}

fn object(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap()
}

fn field(label: &str, value: &str) -> DisplayField {
    DisplayField {
        label: label.to_string(),
        value: value.to_string(),
    }
}

async fn setup(parameters: Vec<WorkflowParameter>, read_only: bool) -> (Harness, String) {
    let h = Harness::new().await;
    let (provider_id, _) = h
        .add_stub(|id| {
            let provider = StubProvider::new(id);
            *provider.parameters.lock().unwrap() = Ok(parameters);
            let mut run = stub_run(&format!("stub__{id}__build"), 3, 60);
            run.branch = Some("main".to_string());
            run.commit_sha = Some("0123456789".to_string());
            run.commit_message = Some("fix things\nbody".to_string());
            run.inputs = Some(json!({ "env": "prod", "ref": "main", "empty": "" }));
            provider.with_runs(vec![run])
        })
        .await;
    if read_only {
        let mut config = h.repository.get_provider(provider_id).await.unwrap();
        config
            .config
            .insert("auth_mode".to_string(), "none".to_string());
        h.repository
            .update_provider(provider_id, &config)
            .await
            .unwrap();
    }
    let target = pipeline(provider_id, "build", PipelineStatus::Success);
    let id = target.id.clone();
    h.repository
        .update_pipelines_cache(provider_id, &[target])
        .await
        .unwrap();
    (h, id)
}

#[tokio::test]
async fn rerun_draft_replays_the_previous_run() {
    let (h, id) = setup(vec![parameter("ref"), parameter("env")], false).await;

    let draft = h
        .pipeline_service
        .prepare_trigger(&id, Some(3), None)
        .await
        .unwrap();

    assert_eq!(
        draft.values,
        object(json!({ "ref": "main", "env": "prod" }))
    );
    assert!(draft.errors.is_empty());
    assert!(draft.availability.allowed);
    let registry = registry();
    let presentation = &registry.get(STUB).unwrap().metadata().presentation;
    assert_eq!(draft.copy, presentation.rerun);
    for expected in [
        field("Pipeline", "build"),
        field("Run", "#3"),
        field("Branch", "main"),
        field("Commit", "0123456 fix things"),
        field("Environment", "prod"),
        field("env", "prod"),
    ] {
        assert!(draft.fields.contains(&expected), "missing {expected:?}");
    }
    assert!(!draft.fields.iter().any(|f| f.label == "ref"));
    assert!(!draft.fields.iter().any(|f| f.label == "empty"));
}

#[tokio::test]
async fn trigger_draft_uses_given_inputs_and_trigger_copy() {
    let (h, id) = setup(vec![parameter("ref")], false).await;

    let draft = h
        .pipeline_service
        .prepare_trigger(&id, None, Some(object(json!({ "ref": "dev" }))))
        .await
        .unwrap();

    assert_eq!(draft.values, object(json!({ "ref": "dev" })));
    let registry = registry();
    let presentation = &registry.get(STUB).unwrap().metadata().presentation;
    assert_eq!(draft.copy, presentation.trigger);
    assert!(!draft.fields.iter().any(|f| f.label == "Run"));
}

#[tokio::test]
async fn draft_reports_missing_required_values() {
    let (h, id) = setup(
        vec![required("ref", Some("Branch name")), required("env", None)],
        false,
    )
    .await;

    let draft = h
        .pipeline_service
        .prepare_trigger(&id, None, Some(object(json!({ "ref": "  " }))))
        .await
        .unwrap();

    assert_eq!(draft.errors["ref"], "Branch name is required");
    assert_eq!(draft.errors["env"], "env is required");
}

#[tokio::test]
async fn read_only_provider_blocks_trigger_and_rerun() {
    let (h, id) = setup(vec![parameter("ref")], true).await;

    let draft = h
        .pipeline_service
        .prepare_trigger(&id, None, None)
        .await
        .unwrap();
    let rerun = h.pipeline_service.rerun_run(&id, 3).await;

    assert!(!draft.availability.allowed);
    assert_eq!(
        draft.availability.reason.as_deref(),
        Some(READ_ONLY_PROVIDER)
    );
    assert!(matches!(rerun, Err(DomainError::NotSupported(m)) if m == READ_ONLY_PROVIDER));
}

#[tokio::test]
async fn inputs_sent_from_pipedash_show_on_the_run_the_provider_matched() {
    let h = Harness::new().await;
    let (provider_id, stub) = h
        .add_stub(|id| {
            let pipeline_id = format!("stub__{id}__build");
            StubProvider::new(id).with_runs(vec![
                stub_run(&pipeline_id, 4, 60),
                stub_run(&pipeline_id, 5, 60),
            ])
        })
        .await;
    let target = pipeline(provider_id, "build", PipelineStatus::Success);
    let id = target.id.clone();
    h.repository
        .update_pipelines_cache(provider_id, &[target])
        .await
        .unwrap();
    let trigger = |run_id: Option<&str>, run_number: i64, env: &str| {
        *stub.trigger_result.lock().unwrap() = Ok(TriggerResult {
            run_id: run_id.map(str::to_string),
            run_number: Some(run_number),
        });
        h.pipeline_service.trigger_pipeline(TriggerParams {
            workflow_id: id.clone(),
            inputs: Some(json!({ "env": env })),
        })
    };

    trigger(Some("run-4"), 4, "prod").await.unwrap();
    trigger(None, 5, "dev").await.unwrap();

    let matched = h.pipeline_service.fetch_run_details(&id, 4).await.unwrap();
    let guessed = h.pipeline_service.fetch_run_details(&id, 5).await.unwrap();
    assert_eq!(matched.inputs, Some(json!({ "env": "prod" })));
    assert_eq!(matched.display.inputs, vec![field("env", "prod")]);
    assert_eq!(guessed.inputs, None);
    assert!(guessed.display.inputs.is_empty());
}

#[test]
fn validation_treats_null_and_blank_as_missing() {
    let parameters = vec![
        required("a", None),
        required("b", None),
        required("c", None),
    ];
    let values = object(json!({ "a": null, "b": " ", "c": false }));

    let errors = validate_params(&parameters, &values);

    assert_eq!(errors.keys().collect::<Vec<_>>(), vec!["a", "b"]);
}
