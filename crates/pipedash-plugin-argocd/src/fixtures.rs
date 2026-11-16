use pipedash_plugin_api::{
    Pipeline,
    PipelineRun,
};
use serde_json::json;

use crate::mapper::{
    self,
    correlation,
    runs,
};
use crate::types::logs::Pod;
use crate::types::Application;

const SERVER_URL: &str = "https://argocd.example.test";
const CLUSTER_UID: &str = "7a1f3c52-0b8e-4d6a-9c11-2f5e8d90ab34";

fn fixture() -> Option<(Pipeline, Vec<PipelineRun>)> {
    let web = "a".repeat(40);
    let config = "b".repeat(40);
    let sources = json!([
        {"repoURL": "https://github.com/acme/shop", "path": "deploy", "targetRevision": "main"},
        {"repoURL": "git@github.com:acme/shop-config.git", "path": "apps/shop"}
    ]);
    let app: Application = serde_json::from_value(json!({
        "metadata": {
            "name": "shop",
            "namespace": "argocd",
            "labels": {"app.kubernetes.io/part-of": "shop"},
            "ownerReferences": [{"kind": "ApplicationSet", "name": "shops"}]
        },
        "spec": {
            "sources": sources,
            "destination": {"server": "https://kubernetes.default.svc", "namespace": "shop"},
            "project": "default"
        },
        "status": {
            "sync": {"status": "Synced", "revisions": [web, config]},
            "health": {"status": "Healthy"},
            "history": [
                {
                    "id": 1,
                    "deployedAt": "2026-10-20T10:00:30Z",
                    "deployStartedAt": "2026-10-20T10:00:00Z",
                    "sources": sources,
                    "revisions": [web, config]
                },
                {
                    "id": 2,
                    "deployedAt": "2026-10-21T10:00:30Z",
                    "sources": sources,
                    "revisions": [config, web]
                }
            ],
            "operationState": {
                "phase": "Succeeded",
                "startedAt": "2026-10-22T10:00:00Z",
                "finishedAt": "2026-10-22T10:00:40Z"
            }
        }
    }))
    .ok()?;
    let pods: Vec<Pod> = serde_json::from_value(json!([{
        "metadata": {"name": "shop-7f9c-abc", "namespace": "shop"},
        "status": {"containerStatuses": [{
            "name": "app",
            "image": "registry.example.test/acme/shop:1.0",
            "imageID": format!("registry.example.test/acme/shop@sha256:{}", "c".repeat(64))
        }]}
    }]))
    .ok()?;

    let mut pipeline = mapper::map_application_to_pipeline(&app, SERVER_URL);
    pipeline.correlation = correlation::pipeline(&app, Some(CLUSTER_UID), Some(&pods));

    Some((pipeline, runs::map_runs(&app, SERVER_URL, 10)))
}

pub fn correlation_fixtures() -> Vec<(Pipeline, Vec<PipelineRun>)> {
    fixture().into_iter().collect()
}

#[cfg(test)]
mod tests;
