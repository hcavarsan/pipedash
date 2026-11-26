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
    let repository: types::Repository = parse(json!({
        "uuid": "{r}", "name": "API", "full_name": "acme/api", "slug": "api",
        "workspace": {"uuid": "{w}", "slug": "acme", "name": "Acme"},
        "links": {"html": {"href": "https://bitbucket.org/acme/api"}}
    }))?;
    let pipelines: Vec<types::Pipeline> = parse(json!([
        {
            "uuid": "{p2}", "build_number": 8,
            "state": {"name": "COMPLETED", "result": {"name": "SUCCESSFUL"}},
            "target": {
                "type": "pipeline_ref_target", "ref_name": "main", "ref_type": "branch",
                "commit": {"hash": COMMIT, "message": "ship it"}
            },
            "created_on": "2026-10-01T10:00:00Z", "completed_on": "2026-10-01T10:05:00Z",
            "duration_in_seconds": 290,
            "links": {"html": {"href": "https://bitbucket.org/acme/api/pipelines/results/8"}}
        },
        {
            "uuid": "{p1}", "build_number": 7,
            "state": {"name": "IN_PROGRESS"},
            "target": {"type": "pipeline_ref_target", "ref_name": "dev", "ref_type": "branch"},
            "created_on": "2026-10-01T09:00:00Z",
            "links": {}
        }
    ]))?;

    let pipeline = mapper::map_pipeline(&repository, pipelines.first());
    let runs = pipelines
        .iter()
        .map(|run| mapper::map_pipeline_run(run, "acme", "api"))
        .collect();
    Some((pipeline, runs))
}

pub fn correlation_fixtures() -> Vec<(Pipeline, Vec<PipelineRun>)> {
    fixture().into_iter().collect()
}
