use pipedash_plugin_api::{
    Pipeline,
    PipelineRun,
};
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::{
    mapper,
    types,
};

pub(crate) const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn parse<T: DeserializeOwned>(value: serde_json::Value) -> Option<T> {
    serde_json::from_value(value).ok()
}

fn fixture() -> Option<(Pipeline, Vec<PipelineRun>)> {
    let project: types::Project = parse(json!({
        "id": 42, "name": "api", "name_with_namespace": "Team / Backend / api",
        "web_url": "https://GitLab.example.com/team/backend/api"
    }))?;
    let runs: Vec<types::Pipeline> = parse(json!([
        {
            "id": 12, "status": "success", "ref": "main", "sha": COMMIT,
            "web_url": "https://gitlab.example.com/team/backend/api/-/pipelines/12",
            "created_at": "2026-10-01T10:00:00Z", "updated_at": "2026-10-01T10:10:00Z",
            "started_at": "2026-10-01T10:01:00Z", "finished_at": "2026-10-01T10:10:00Z",
            "duration": 540
        },
        {
            "id": 11, "status": "canceled", "ref": "main", "sha": COMMIT,
            "web_url": "https://gitlab.example.com/team/backend/api/-/pipelines/11",
            "created_at": "2026-10-01T09:00:00Z", "updated_at": "2026-10-01T09:05:00Z",
            "started_at": null, "finished_at": "2026-10-01T09:05:00Z", "duration": null
        }
    ]))?;
    let deployments: Vec<types::Deployment> = parse(json!([
        {"environment": {"name": "production"}, "deployable": {"pipeline": {"id": 12}}},
        {"environment": {"name": "review/fix-foo"}, "deployable": {"pipeline": {"id": 12}}},
        {"environment": {"name": "production"}, "deployable": {"pipeline": {"id": 12}}},
        {"environment": {"name": "staging"}, "deployable": null},
        {"environment": {"name": "staging"}, "deployable": {"pipeline": {"id": 99}}}
    ]))?;
    let environments: Vec<types::Environment> = parse(json!([
        {"name": "production", "auto_stop_at": null},
        {"name": "review/fix-foo", "auto_stop_at": "2026-10-02T10:00:00Z"}
    ]))?;

    let mut targets = mapper::targets::run_targets(&project, &deployments, &environments);
    let pipeline = mapper::map_pipeline(&project, runs.first());
    let runs = runs
        .iter()
        .map(|run| {
            let run_targets = targets.remove(&run.id).unwrap_or_default();
            mapper::map_pipeline_run(run, &project, None, run_targets)
        })
        .collect();
    Some((pipeline, runs))
}

pub fn correlation_fixtures() -> Vec<(Pipeline, Vec<PipelineRun>)> {
    fixture().into_iter().collect()
}
