use pipedash_plugin_api::{
    Pipeline,
    PipelineRun,
};
use reqwest::Url;
use serde_json::json;

use crate::{
    config,
    mapper,
    types,
};

const PROJECT: &str = "Web App";

pub fn correlation_fixtures() -> Vec<(Pipeline, Vec<PipelineRun>)> {
    fixture().into_iter().collect()
}

fn fixture() -> Option<(Pipeline, Vec<PipelineRun>)> {
    let repository = json!({"url": "https://dev.azure.com/acme/Web%20App/_git/shop"});
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let definition: types::Definition = serde_json::from_value(json!({
        "id": 12,
        "name": "CI",
        "latestBuild": {
            "id": 900,
            "status": "completed",
            "result": "succeeded",
            "sourceBranch": "refs/heads/main",
            "sourceVersion": sha,
            "startTime": "2026-10-28T10:00:00Z",
            "finishTime": "2026-10-28T10:03:00Z",
            "repository": repository
        }
    }))
    .ok()?;
    let build: types::Build = serde_json::from_value(json!({
        "id": 900,
        "status": "completed",
        "result": "succeeded",
        "sourceBranch": "refs/heads/main",
        "sourceVersion": sha,
        "startTime": "2026-10-28T10:00:00Z",
        "finishTime": "2026-10-28T10:03:00Z",
        "repository": repository
    }))
    .ok()?;
    let server = Url::parse("https://dev.azure.com/acme/").ok()?;
    let pipeline = mapper::map_pipeline("acme", PROJECT, &definition);
    let run = mapper::map_run(
        &build,
        &config::build_pipeline_id(PROJECT, definition.id),
        PROJECT,
        &server,
    );
    Some((pipeline, vec![run]))
}
