use pipedash_plugin_api::{
    Pipeline,
    PipelineRun,
    RepoId,
};
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::{
    config,
    mapper,
    types,
};

pub(crate) const HOST: &str = "gitea.example.com";
pub(crate) const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn parse<T: DeserializeOwned>(value: serde_json::Value) -> Option<T> {
    serde_json::from_value(value).ok()
}

fn fixture() -> Option<(Pipeline, Vec<PipelineRun>)> {
    let workflow: types::Workflow =
        parse(json!({"id": "ci.yaml", "name": "ci", "path": ".gitea/workflows/ci.yaml"}))?;
    let runs: Vec<types::Run> = parse(json!([
        {
            "id": 91, "run_number": 5, "status": "completed", "conclusion": "success",
            "path": "ci.yaml@refs/heads/main", "event": "push", "display_title": "ship it",
            "head_branch": "main", "head_sha": COMMIT,
            "html_url": "https://gitea.example.com/acme/api/actions/runs/5",
            "started_at": "2026-10-01T10:00:00Z", "completed_at": "2026-10-01T10:04:00Z"
        },
        {
            "id": 90, "run_number": 4, "status": "completed", "conclusion": "cancelled",
            "path": "ci.yaml@refs/heads/main", "event": "push", "display_title": "wip",
            "head_branch": "main", "head_sha": COMMIT,
            "html_url": "https://gitea.example.com/acme/api/actions/runs/4",
            "started_at": "1970-01-01T00:00:00Z", "completed_at": "2026-10-01T09:00:00Z"
        }
    ]))?;

    let source = RepoId::from_host_path(HOST, "acme/api");
    let pipeline_id = config::build_pipeline_id("acme", "api", &workflow.id);
    let pipeline = mapper::map_pipeline("acme", "api", &workflow, runs.first(), source.clone());
    let runs = runs
        .iter()
        .map(|run| mapper::map_run(run, &pipeline_id, "acme", source.clone()))
        .collect();
    Some((pipeline, runs))
}

pub fn correlation_fixtures() -> Vec<(Pipeline, Vec<PipelineRun>)> {
    fixture().into_iter().collect()
}
