use pipedash_plugin_api::{
    Pipeline,
    PipelineRun,
};
use serde_json::{
    from_value,
    json,
    Value,
};

use crate::mapper::{
    map_pipeline,
    map_run,
    Placement,
};
use crate::types::{
    Application,
    Deployment,
};

const SERVER_URL: &str = "https://coolify.test";
const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn applications() -> [(Value, &'static str); 2] {
    [
        (
            json!({
                "uuid": "app-web", "name": "web", "status": "running:healthy",
                "build_pack": "nixpacks", "git_repository": "git@github.com:acme/web.git",
                "git_branch": "main", "environment_id": 1,
                "destination": {"server": {"name": "prod-1"}}
            }),
            "production",
        ),
        (
            json!({
                "uuid": "app-img", "name": "img", "status": "running:healthy",
                "build_pack": "dockerimage", "git_repository": "coollabsio/coolify",
                "docker_registry_image_name": "ghcr.io/acme/img",
                "docker_registry_image_tag": "1.2", "environment_id": 2,
                "destination": {"server": {"name": "prod-1"}}
            }),
            "staging",
        ),
    ]
}

fn deployments() -> Value {
    json!([
        {
            "id": 2, "deployment_uuid": "d2", "status": "finished", "commit": SHA,
            "created_at": "2026-11-01T10:10:00Z", "finished_at": "2026-11-01T10:12:00Z",
            "deployment_url": "/project/p/environment/e/application/a/deployment/d2"
        },
        {
            "id": 1, "deployment_uuid": "d1", "status": "failed", "commit": "HEAD",
            "created_at": "2026-11-01T09:00:00Z", "finished_at": "2026-11-01T09:01:00Z"
        }
    ])
}

fn fixture(app: Value, environment: &str) -> Option<(Pipeline, Vec<PipelineRun>)> {
    let app: Application = from_value(app).ok()?;
    let deployments: Vec<Deployment> = from_value(deployments()).ok()?;
    let placement = Placement {
        project: "shop".to_string(),
        environment: environment.to_string(),
    };
    let pipeline = map_pipeline(&app, deployments.first(), Some(&placement));
    let runs = deployments
        .iter()
        .map(|deployment| map_run(&pipeline.id, SERVER_URL, deployment, Some(&app)))
        .collect();
    Some((pipeline, runs))
}

pub fn correlation_fixtures() -> Vec<(Pipeline, Vec<PipelineRun>)> {
    applications()
        .into_iter()
        .filter_map(|(app, environment)| fixture(app, environment))
        .collect()
}

#[cfg(test)]
mod tests;
