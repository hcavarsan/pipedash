use pipedash_plugin_api::utils::repository_name;
use pipedash_plugin_api::AvailablePipeline;

use crate::{
    config,
    types,
};

pub(crate) fn map_available_pipeline(app: &types::Application) -> AvailablePipeline {
    let app_name = app.metadata.name.clone();
    let project_name = app.spec.project.clone();
    let source = app.spec.primary_source();

    let repo_full_name = repository_name(&source.repo_url);

    let git_org_raw = config::extract_git_org(&source.repo_url);

    let (git_org, repo_name, unique_id) = if git_org_raw == "unknown" || git_org_raw.is_empty() {
        (None, None, app_name.clone())
    } else {
        (Some(git_org_raw), Some(repo_full_name), app_name.clone())
    };

    let cluster = app.spec.destination.cluster();
    let cluster_name = cluster
        .split("//")
        .last()
        .unwrap_or(cluster)
        .split(':')
        .next()
        .unwrap_or("unknown");

    let description = format!(
        "Project: {} | Path: {} | Target: {} | Namespace: {} | Cluster: {}",
        project_name,
        source.path.as_deref().unwrap_or("/"),
        source.target_revision.as_deref().unwrap_or("HEAD"),
        app.spec.destination.namespace.as_deref().unwrap_or("-"),
        cluster_name
    );

    AvailablePipeline {
        id: unique_id,
        name: app_name,
        description: Some(description),
        organization: git_org,
        repository: repo_name,
    }
}
