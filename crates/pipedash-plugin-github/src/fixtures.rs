use octocrab::models::workflows::WorkFlow;
use pipedash_plugin_api::{
    ClusterId,
    Pipeline,
    PipelineRun,
    RepoId,
};

use crate::correlation::paths;
use crate::{
    correlation,
    mapper,
    types,
};

const WORKFLOW_SOURCE: &str = "name: CI\non:\n  push:\n    branches: [main]\n    paths:\n      - 'services/api/**'\n      - 'Cargo.toml'\n  pull_request:\n    paths:\n      - 'services/api/**'\n      - '!services/api/docs/**'\njobs: {}\n";

fn workflow() -> serde_json::Value {
    let url = "https://github.test/acme/app";
    serde_json::json!({
        "id": 42, "node_id": "n", "name": "CI", "path": ".github/workflows/ci.yml",
        "state": "active", "created_at": "2026-10-20T10:00:00Z",
        "updated_at": "2026-10-20T10:00:00Z", "url": url, "html_url": url, "badge_url": url
    })
}

fn run() -> serde_json::Value {
    let url = "https://github.test/acme/app";
    serde_json::json!({
        "id": 9001, "workflow_id": 42, "node_id": "n", "name": "CI",
        "head_branch": "main", "head_sha": "0123456789abcdef0123456789abcdef01234567",
        "run_number": 7, "event": "push", "status": "completed", "conclusion": "success",
        "created_at": "2026-10-20T10:00:00Z", "updated_at": "2026-10-20T10:10:00Z",
        "url": url, "html_url": url, "jobs_url": url, "logs_url": url,
        "check_suite_url": url, "artifacts_url": url, "cancel_url": url,
        "rerun_url": url, "workflow_url": url,
        "head_commit": {
            "id": "0123456789abcdef0123456789abcdef01234567", "tree_id": "def",
            "message": "ship it", "timestamp": "2026-10-20T09:59:00Z",
            "author": {"name": "Ada", "email": "ada@example.com"},
            "committer": {"name": "Ada", "email": "ada@example.com"}
        },
        "repository": {"id": 1, "name": "app", "url": url}
    })
}

fn deployments() -> serde_json::Value {
    serde_json::json!([
        {"environment": "production", "created_at": "2026-10-20T10:05:00Z"},
        {
            "environment": "preview-7", "created_at": "2026-10-20T10:04:00Z",
            "transient_environment": true
        },
        {"environment": "staging", "created_at": "2026-10-19T10:04:00Z"}
    ])
}

pub fn correlation_fixtures() -> Vec<(Pipeline, Vec<PipelineRun>)> {
    let (Ok(workflow), Ok(run), Ok(deployments)) = (
        serde_json::from_value::<WorkFlow>(workflow()),
        serde_json::from_value::<types::Run>(run()),
        serde_json::from_value::<Vec<types::Deployment>>(deployments()),
    ) else {
        return Vec::new();
    };

    let repo = RepoId::from_host_path("github.test", "acme/app");
    let targets = ClusterId::from_host("github.test/acme/app")
        .map(|cluster| {
            correlation::deployment_targets(&cluster, run.created_at, run.updated_at, &deployments)
        })
        .unwrap_or_default();
    let pipeline = mapper::workflow_pipeline(
        "acme",
        "app",
        "acme/app",
        workflow,
        Some(&run),
        correlation::pipeline_correlation(repo.clone(), paths::trigger_scopes(WORKFLOW_SOURCE)),
    );
    let runs = vec![mapper::run_to_pipeline_run(
        run,
        &pipeline.id,
        repo,
        targets,
    )];
    vec![(pipeline, runs)]
}

#[cfg(test)]
mod tests;
