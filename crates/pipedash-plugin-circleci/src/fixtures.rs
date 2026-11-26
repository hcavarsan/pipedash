use pipedash_plugin_api::{
    Pipeline,
    PipelineRun,
};
use serde_json::json;

use crate::mapper;
use crate::types::{
    CirclePipeline,
    Project,
    Workflow,
};

pub fn correlation_fixtures() -> Vec<(Pipeline, Vec<PipelineRun>)> {
    fixture().into_iter().collect()
}

fn fixture() -> Option<(Pipeline, Vec<PipelineRun>)> {
    let pipeline: CirclePipeline = serde_json::from_value(json!({
        "id": "pipeline-id",
        "number": 42,
        "state": "created",
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:01:00Z",
        "vcs": {
            "branch": "main",
            "revision": "0123456789abcdef0123456789abcdef01234567",
            "target_repository_url": "https://github.com/acme/shop",
            "commit": {"subject": "Fix"}
        },
        "trigger": {"actor": {"login": "dev"}}
    }))
    .ok()?;
    let workflow: Workflow = serde_json::from_value(json!({
        "id": "workflow-id",
        "name": "build",
        "status": "success",
        "created_at": "2026-01-01T00:00:10Z",
        "stopped_at": "2026-01-01T00:00:50Z"
    }))
    .ok()?;
    let project = Project {
        slug: "gh/acme/shop".to_string(),
        id: None,
        name: "shop".to_string(),
        organization: "acme".to_string(),
        repository_url: "https://github.com/acme/shop".to_string(),
        default_branch: Some("main".to_string()),
    };
    let run = mapper::to_run(
        &project.slug,
        &pipeline,
        &[workflow],
        "https://circleci.com",
        &project.slug,
    );
    Some((mapper::to_pipeline(&project, Some(&run)), vec![run]))
}
