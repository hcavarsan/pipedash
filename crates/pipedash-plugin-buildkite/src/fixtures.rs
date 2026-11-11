use pipedash_plugin_api::{
    Pipeline,
    PipelineRun,
};
use serde_json::json;

use crate::{
    mapper,
    types,
};

pub fn correlation_fixtures() -> Vec<(Pipeline, Vec<PipelineRun>)> {
    fixture().into_iter().collect()
}

fn fixture() -> Option<(Pipeline, Vec<PipelineRun>)> {
    let repository = "git@github.com:acme/shop.git";
    let pipeline: types::Pipeline = serde_json::from_value(json!({
        "name": "Shop",
        "repository": repository
    }))
    .ok()?;
    let build: types::Build = serde_json::from_value(json!({
        "id": "build-id",
        "number": 42,
        "state": "passed",
        "web_url": "https://buildkite.com/acme/shop/builds/42",
        "branch": "main",
        "commit": "0123456789abcdef0123456789abcdef01234567",
        "created_at": "2026-10-28T10:00:00Z",
        "started_at": "2026-10-28T10:00:05Z",
        "finished_at": "2026-10-28T10:03:00Z",
        "pipeline": {"repository": repository}
    }))
    .ok()?;
    let mapped = mapper::map_pipeline("acme", "shop", pipeline, Some(&build));
    let run = mapper::build_to_pipeline_run(build, &mapped.id);
    Some((mapped, vec![run]))
}
