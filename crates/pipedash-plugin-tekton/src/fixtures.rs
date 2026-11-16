use pipedash_plugin_api::{
    Pipeline,
    PipelineRun,
};

use crate::mapper::map_pipeline;
use crate::mapper::runs::map_pipeline_run;
use crate::types::{
    TektonPipeline,
    TektonPipelineRun,
    TektonTaskRun,
};

pub(crate) const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
pub(crate) const DIGEST_A: &str =
    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub(crate) const DIGEST_B: &str =
    "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

pub(crate) fn pipeline_json() -> serde_json::Value {
    serde_json::json!({
        "apiVersion": "tekton.dev/v1",
        "kind": "Pipeline",
        "metadata": {"name": "build", "namespace": "ci"},
        "spec": {}
    })
}

pub(crate) fn run_json() -> serde_json::Value {
    serde_json::json!({
        "apiVersion": "tekton.dev/v1",
        "kind": "PipelineRun",
        "metadata": {
            "name": "build-1",
            "namespace": "ci",
            "creationTimestamp": "2026-01-01T10:00:00Z",
            "annotations": {
                "pipelinesascode.tekton.dev/repo-url": "https://Git.Test/shop/api",
                "pipelinesascode.tekton.dev/sha": SHA
            },
            "labels": {"tekton.dev/pipeline": "build"}
        },
        "spec": {"pipelineRef": {"name": "build"}},
        "status": {
            "startTime": "2026-01-01T10:00:00Z",
            "completionTime": "2026-01-01T10:05:00Z",
            "conditions": [{"type": "Succeeded", "status": "True"}],
            "results": [{
                "name": "IMAGES",
                "value": format!("reg.test/shop/web@{DIGEST_B}\nnot-an-image-with-no-digest:latest")
            }]
        }
    })
}

pub(crate) fn task_run_json() -> serde_json::Value {
    serde_json::json!({
        "metadata": {
            "name": "build-1-push",
            "namespace": "ci",
            "labels": {"tekton.dev/pipelineRun": "build-1"}
        },
        "status": {"results": [
            {"name": "IMAGE_URL", "value": "reg.test/shop/api:v1"},
            {"name": "IMAGE_DIGEST", "value": DIGEST_A}
        ]}
    })
}

pub fn correlation_fixtures() -> Vec<(Pipeline, Vec<PipelineRun>)> {
    let (Ok(pipeline), Ok(run), Ok(task)) = (
        serde_json::from_value::<TektonPipeline>(pipeline_json()),
        serde_json::from_value::<TektonPipelineRun>(run_json()),
        serde_json::from_value::<TektonTaskRun>(task_run_json()),
    ) else {
        return Vec::new();
    };
    vec![(
        map_pipeline(&pipeline, Some(&run)),
        vec![map_pipeline_run(&run, &[task])],
    )]
}
