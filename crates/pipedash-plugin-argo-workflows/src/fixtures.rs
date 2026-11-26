use pipedash_plugin_api::{
    Pipeline,
    PipelineRun,
};

use crate::config::{
    Kind,
    PipelineRef,
};
use crate::mapper::{
    map_pipeline,
    map_run,
};
use crate::types::Workflow;

fn workflow_json(name: &str, started: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "metadata": {
            "name": name,
            "namespace": "ci",
            "uid": "0000002a-1111-2222-3333-444444444444",
            "creationTimestamp": "2026-01-01T10:00:00Z"
        },
        "status": {
            "phase": if started.is_some() { "Succeeded" } else { "Pending" },
            "startedAt": started,
            "finishedAt": started.map(|_| "2026-01-01T10:05:00Z")
        }
    })
}

pub fn correlation_fixtures() -> Vec<(Pipeline, Vec<PipelineRun>)> {
    let (Ok(finished), Ok(waiting)) = (
        serde_json::from_value::<Workflow>(workflow_json("build-1", Some("2026-01-01T10:00:05Z"))),
        serde_json::from_value::<Workflow>(workflow_json("build-2", None)),
    ) else {
        return Vec::new();
    };
    let pipeline = PipelineRef::new("ci", Kind::Template, "build");
    vec![(
        map_pipeline(&pipeline, None, Some(&finished)),
        vec![map_run(&pipeline, &finished), map_run(&pipeline, &waiting)],
    )]
}
