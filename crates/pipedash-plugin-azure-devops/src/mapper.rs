use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    AvailablePipeline,
    Pipeline,
    PipelineRun,
    PipelineStatus,
};
use reqwest::Url;
use serde_json::{
    json,
    Map,
    Value,
};

use crate::{
    config,
    types,
};

pub(crate) const BRANCH_INPUT: &str = "source-branch";
const REF_ECHO_INPUT: &str = "ref";
const BRANCH_REF_PREFIX: &str = "refs/heads/";
const REF_PREFIX: &str = "refs/";

pub(crate) fn map_status(status: Option<&str>, result: Option<&str>) -> PipelineStatus {
    match (status, result) {
        (Some("completed"), Some("succeeded")) => PipelineStatus::Success,
        (Some("completed"), Some("canceled")) => PipelineStatus::Cancelled,
        (Some("completed"), Some("partiallySucceeded")) => PipelineStatus::Failed,
        (Some("completed"), _) => PipelineStatus::Failed,
        (Some("inProgress" | "cancelling"), _) => PipelineStatus::Running,
        _ => PipelineStatus::Pending,
    }
}

fn map_build_status(build: &types::Build) -> PipelineStatus {
    map_status(build.status.as_deref(), build.result.as_deref())
}

fn timestamp(value: Option<DateTime<Utc>>) -> Option<DateTime<Utc>> {
    value.filter(|t| *t > DateTime::UNIX_EPOCH)
}

pub(crate) fn short_branch(source_branch: &str) -> &str {
    source_branch
        .strip_prefix(BRANCH_REF_PREFIX)
        .unwrap_or(source_branch)
}

fn branch_of(build: &types::Build) -> Option<String> {
    build
        .source_branch
        .as_deref()
        .filter(|branch| !branch.is_empty())
        .map(|branch| short_branch(branch).to_string())
}

pub(crate) fn map_available_pipeline(
    project: &types::Project, organization: &str,
) -> AvailablePipeline {
    AvailablePipeline {
        id: project.name.clone(),
        name: project.name.clone(),
        description: project.description.clone().filter(|d| !d.is_empty()),
        organization: Some(organization.to_string()),
        repository: None,
    }
}

pub(crate) fn map_pipeline(
    provider_id: i64, organization: &str, project: &str, definition: &types::Definition,
) -> Pipeline {
    let latest_build = definition.latest_build.as_ref();

    Pipeline {
        id: config::build_pipeline_id(provider_id, project, definition.id),
        provider_id,
        provider_type: config::PROVIDER_TYPE.to_string(),
        name: definition.name.clone(),
        status: latest_build
            .map(map_build_status)
            .unwrap_or(PipelineStatus::Pending),
        last_run: latest_build
            .and_then(|build| timestamp(build.start_time).or_else(|| timestamp(build.queue_time))),
        last_updated: Utc::now(),
        repository: format!("{organization}/{project}"),
        branch: latest_build.and_then(branch_of),
        workflow_file: None,
        metadata: HashMap::new(),
    }
}

fn run_inputs(build: &types::Build, branch: Option<String>) -> Option<Value> {
    let mut inputs = Map::new();
    if let Some(branch) = branch {
        inputs.insert(BRANCH_INPUT.to_string(), Value::String(branch));
    }
    if let Some(Ok(Value::Object(variables))) = build
        .parameters
        .as_deref()
        .map(serde_json::from_str::<Value>)
    {
        inputs.extend(variables);
    }
    (!inputs.is_empty()).then_some(Value::Object(inputs))
}

pub(crate) fn map_run(
    build: &types::Build, pipeline_id: &str, project: &str, server_url: &Url,
) -> PipelineRun {
    let started_at = timestamp(build.start_time);
    let concluded_at = timestamp(build.finish_time);
    let branch = branch_of(build);

    PipelineRun {
        id: format!("{}-run-{}", config::PROVIDER_TYPE, build.id),
        pipeline_id: pipeline_id.to_string(),
        run_number: build.id,
        status: map_build_status(build),
        started_at: started_at
            .or_else(|| timestamp(build.queue_time))
            .unwrap_or_else(Utc::now),
        concluded_at,
        duration_seconds: started_at
            .zip(concluded_at)
            .map(|(started, concluded)| (concluded - started).num_seconds()),
        logs_url: build
            .web_link()
            .map(str::to_string)
            .unwrap_or_else(|| config::build_web_url(server_url, project, build.id)),
        commit_sha: build.source_version.clone().filter(|sha| !sha.is_empty()),
        commit_message: None,
        branch: branch.clone(),
        actor: build
            .requested_for
            .as_ref()
            .and_then(|identity| identity.display_name.clone()),
        inputs: run_inputs(build, branch),
        metadata: build
            .build_number
            .iter()
            .map(|number| ("build_number".to_string(), json!(number)))
            .collect(),
    }
}

pub(crate) fn source_branch(branch: &str) -> String {
    if branch.starts_with(REF_PREFIX) {
        branch.to_string()
    } else {
        format!("{BRANCH_REF_PREFIX}{branch}")
    }
}

pub(crate) fn trigger_body(definition_id: i64, inputs: Option<Value>) -> Value {
    let mut branch = None;
    let mut variables = Map::new();
    if let Some(Value::Object(inputs)) = inputs {
        for (name, value) in inputs {
            let text = match value {
                Value::String(text) => text,
                Value::Bool(flag) => flag.to_string(),
                Value::Number(number) => number.to_string(),
                _ => continue,
            };
            if text.trim().is_empty() {
                continue;
            }
            if name == BRANCH_INPUT {
                branch = Some(text);
            } else {
                variables.insert(name, Value::String(text));
            }
        }
    }
    if let Some(branch) = &branch {
        if variables.get(REF_ECHO_INPUT).and_then(Value::as_str) == Some(branch.as_str()) {
            variables.remove(REF_ECHO_INPUT);
        }
    }

    let mut body = json!({ "definition": { "id": definition_id } });
    if let Some(branch) = branch {
        body["sourceBranch"] = Value::String(source_branch(branch.trim()));
    }
    if !variables.is_empty() {
        body["parameters"] = Value::String(Value::Object(variables).to_string());
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUSES: [&str; 7] = [
        "none",
        "inProgress",
        "completed",
        "cancelling",
        "postponed",
        "notStarted",
        "all",
    ];
    const RESULTS: [&str; 5] = [
        "none",
        "succeeded",
        "partiallySucceeded",
        "failed",
        "canceled",
    ];

    fn server() -> Url {
        Url::parse("https://dev.azure.com/acme/").unwrap()
    }

    fn build(json: &str) -> types::Build {
        serde_json::from_str(json).unwrap()
    }

    fn sorted(words: impl IntoIterator<Item = &'static str>) -> Vec<&'static str> {
        let mut words: Vec<_> = words.into_iter().collect();
        words.sort_unstable();
        words
    }

    #[test]
    fn completed_builds_map_by_result() {
        let cases = [
            ("succeeded", PipelineStatus::Success),
            ("partiallySucceeded", PipelineStatus::Failed),
            ("failed", PipelineStatus::Failed),
            ("canceled", PipelineStatus::Cancelled),
            ("none", PipelineStatus::Failed),
        ];
        for (result, expected) in cases {
            assert_eq!(
                map_status(Some("completed"), Some(result)),
                expected,
                "completed/{result}"
            );
        }
        assert_eq!(map_status(Some("completed"), None), PipelineStatus::Failed);
        assert_eq!(
            sorted(cases.iter().map(|(result, _)| *result)),
            sorted(RESULTS)
        );
    }

    #[test]
    fn unfinished_builds_map_by_status_whatever_the_result() {
        let cases = [
            ("none", PipelineStatus::Pending),
            ("inProgress", PipelineStatus::Running),
            ("cancelling", PipelineStatus::Running),
            ("postponed", PipelineStatus::Pending),
            ("notStarted", PipelineStatus::Pending),
            ("all", PipelineStatus::Pending),
        ];
        for (status, expected) in cases {
            for result in RESULTS {
                assert_eq!(
                    map_status(Some(status), Some(result)),
                    expected,
                    "{status}/{result}"
                );
            }
            assert_eq!(map_status(Some(status), None), expected, "{status}");
        }
        assert_eq!(
            sorted(cases.iter().map(|(status, _)| *status).chain(["completed"])),
            sorted(STATUSES)
        );
    }

    #[test]
    fn missing_or_unknown_status_words_stay_pending() {
        assert_eq!(map_status(None, None), PipelineStatus::Pending);
        assert_eq!(
            map_status(Some("somethingNew"), Some("succeeded")),
            PipelineStatus::Pending
        );
    }

    #[test]
    fn definition_takes_its_status_branch_and_time_from_the_latest_build() {
        let definition: types::Definition = serde_json::from_str(
            r#"{
                "id": 12, "name": "CI",
                "latestBuild": {
                    "id": 900, "status": "completed", "result": "succeeded",
                    "sourceBranch": "refs/heads/release/1.0",
                    "queueTime": "2026-10-28T09:59:00.1234567Z",
                    "startTime": "2026-10-28T10:00:00.1234567Z"
                }
            }"#,
        )
        .unwrap();

        let pipeline = map_pipeline(3, "acme", "Web App", &definition);

        assert_eq!(pipeline.id, "azure-devops__3__Web App/12");
        assert_eq!(pipeline.provider_type, "azure-devops");
        assert_eq!(pipeline.name, "CI");
        assert_eq!(pipeline.status, PipelineStatus::Success);
        assert_eq!(pipeline.repository, "acme/Web App");
        assert_eq!(pipeline.branch.as_deref(), Some("release/1.0"));
        assert_eq!(
            pipeline.last_run.unwrap().to_rfc3339(),
            "2026-10-28T10:00:00.123456700+00:00"
        );
    }

    #[test]
    fn definition_without_builds_is_pending() {
        let definition: types::Definition =
            serde_json::from_str(r#"{"id": 1, "name": "New"}"#).unwrap();
        let pipeline = map_pipeline(1, "acme", "App", &definition);
        assert_eq!(pipeline.status, PipelineStatus::Pending);
        assert_eq!(pipeline.last_run, None);
        assert_eq!(pipeline.branch, None);
    }

    #[test]
    fn run_carries_branch_commit_actor_times_link_and_variables() {
        let run = build(
            r#"{
                "id": 901, "buildNumber": "20261028.2",
                "status": "completed", "result": "failed",
                "sourceBranch": "refs/heads/main", "sourceVersion": "abc123def",
                "queueTime": "2026-10-28T09:59:50Z",
                "startTime": "2026-10-28T10:00:00Z", "finishTime": "2026-10-28T10:02:30Z",
                "requestedFor": {"displayName": "Ada Lovelace"},
                "parameters": "{\"deployTarget\":\"staging\"}",
                "_links": {"web": {"href": "https://dev.azure.com/acme/App/_build/results?buildId=901"}}
            }"#,
        );

        let mapped = map_run(&run, "azure-devops__1__App/12", "App", &server());

        assert_eq!(mapped.id, "azure-devops-run-901");
        assert_eq!(mapped.run_number, 901);
        assert_eq!(mapped.status, PipelineStatus::Failed);
        assert_eq!(mapped.branch.as_deref(), Some("main"));
        assert_eq!(mapped.commit_sha.as_deref(), Some("abc123def"));
        assert_eq!(mapped.actor.as_deref(), Some("Ada Lovelace"));
        assert_eq!(mapped.duration_seconds, Some(150));
        assert!(mapped.concluded_at.is_some());
        assert_eq!(
            mapped.logs_url,
            "https://dev.azure.com/acme/App/_build/results?buildId=901"
        );
        assert_eq!(
            mapped.inputs,
            Some(json!({"source-branch": "main", "deployTarget": "staging"}))
        );
        assert_eq!(mapped.metadata["build_number"], json!("20261028.2"));
    }

    #[test]
    fn run_keeps_non_branch_refs_whole() {
        let run = build(r#"{"id": 5, "sourceBranch": "refs/pull/17/merge"}"#);
        let mapped = map_run(&run, "azure-devops__1__App/12", "App", &server());
        assert_eq!(mapped.branch.as_deref(), Some("refs/pull/17/merge"));
    }

    #[test]
    fn run_with_only_an_id_still_maps() {
        let run = build(r#"{"id": 5}"#);
        let mapped = map_run(&run, "azure-devops__1__My App/12", "My App", &server());

        assert_eq!(mapped.status, PipelineStatus::Pending);
        assert_eq!(mapped.concluded_at, None);
        assert_eq!(mapped.duration_seconds, None);
        assert_eq!(mapped.branch, None);
        assert_eq!(mapped.commit_sha, None);
        assert_eq!(mapped.actor, None);
        assert_eq!(mapped.inputs, None);
        assert!(mapped.metadata.is_empty());
        assert_eq!(
            mapped.logs_url,
            "https://dev.azure.com/acme/My%20App/_build/results?buildId=5"
        );
    }

    #[test]
    fn unset_minimum_date_timestamps_are_treated_as_missing() {
        let run = build(
            r#"{"id": 6, "status": "notStarted", "queueTime": "2026-10-28T09:00:00Z",
                "startTime": "0001-01-01T00:00:00Z", "finishTime": "0001-01-01T00:00:00Z"}"#,
        );
        let mapped = map_run(&run, "azure-devops__1__App/12", "App", &server());

        assert_eq!(mapped.started_at.to_rfc3339(), "2026-10-28T09:00:00+00:00");
        assert_eq!(mapped.concluded_at, None);
        assert_eq!(mapped.duration_seconds, None);
    }

    #[test]
    fn unparsable_variable_parameters_are_ignored() {
        let run = build(r#"{"id": 7, "sourceBranch": "refs/heads/dev", "parameters": "oops"}"#);
        let mapped = map_run(&run, "azure-devops__1__App/12", "App", &server());
        assert_eq!(mapped.inputs, Some(json!({"source-branch": "dev"})));
    }

    #[test]
    fn a_build_variable_named_branch_does_not_overwrite_the_saved_branch() {
        let run = build(
            r#"{"id": 8, "sourceBranch": "refs/heads/main", "parameters": "{\"branch\":\"legacy\"}"}"#,
        );
        let mapped = map_run(&run, "azure-devops__1__App/12", "App", &server());
        assert_eq!(
            mapped.inputs,
            Some(json!({"source-branch": "main", "branch": "legacy"}))
        );
    }

    #[test]
    fn project_becomes_a_selectable_item_of_the_organization() {
        let project = types::Project {
            name: "Web App".to_string(),
            description: Some(String::new()),
        };
        let available = map_available_pipeline(&project, "acme");
        assert_eq!(available.id, "Web App");
        assert_eq!(available.organization.as_deref(), Some("acme"));
        assert_eq!(available.description, None);
    }

    #[test]
    fn trigger_body_carries_branch_ref_and_variable_overrides() {
        let body = trigger_body(
            12,
            Some(
                json!({"source-branch": "feature/x", "deployTarget": "staging", "retries": 3, "fast": true}),
            ),
        );

        assert_eq!(body["definition"], json!({"id": 12}));
        assert_eq!(body["sourceBranch"], "refs/heads/feature/x");
        let parameters: Value = serde_json::from_str(body["parameters"].as_str().unwrap()).unwrap();
        assert_eq!(
            parameters,
            json!({"deployTarget": "staging", "retries": "3", "fast": "true"})
        );
    }

    #[test]
    fn trigger_body_keeps_full_refs_and_drops_blank_inputs() {
        let body = trigger_body(
            12,
            Some(json!({"source-branch": "refs/tags/v1", "empty": "  ", "gone": null})),
        );
        assert_eq!(body["sourceBranch"], "refs/tags/v1");
        assert!(body.get("parameters").is_none());

        assert_eq!(trigger_body(12, None), json!({"definition": {"id": 12}}));
        assert_eq!(
            trigger_body(12, Some(json!({"source-branch": ""}))),
            json!({"definition": {"id": 12}})
        );
    }

    #[test]
    fn trigger_body_keeps_a_variable_named_branch_apart_from_the_source_branch() {
        let body = trigger_body(
            12,
            Some(json!({"source-branch": "main", "branch": "legacy"})),
        );
        assert_eq!(body["sourceBranch"], "refs/heads/main");
        assert_eq!(body["parameters"], r#"{"branch":"legacy"}"#);
    }

    #[test]
    fn trigger_body_ignores_the_ref_input_the_rerun_flow_adds_for_the_branch() {
        let body = trigger_body(12, Some(json!({"source-branch": "main", "ref": "main"})));
        assert_eq!(body["sourceBranch"], "refs/heads/main");
        assert!(body.get("parameters").is_none());

        let kept = trigger_body(12, Some(json!({"source-branch": "main", "ref": "other"})));
        assert_eq!(kept["parameters"], r#"{"ref":"other"}"#);
    }
}
