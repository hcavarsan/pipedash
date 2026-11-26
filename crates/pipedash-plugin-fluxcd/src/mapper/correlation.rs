use pipedash_plugin_api::{
    ArtifactObservation,
    ClusterId,
    CommitSha,
    ConsumedArtifact,
    Correlation,
    PathScope,
    PipelineRun,
    RepoId,
    SourceObservation,
    TargetLifetime,
    TargetObservation,
    UnitId,
};

use super::metadata::{
    current_revision,
    source_reference,
    split_revision,
};
use crate::types::{
    FluxResource,
    Kind,
    Reference,
    GIT_REPOSITORY,
};

const LABEL_PART_OF: &str = "app.kubernetes.io/part-of";
const LABEL_NAME: &str = "app.kubernetes.io/name";

#[derive(Debug, Default)]
pub(crate) struct Facts {
    pub repo: Option<RepoId>,
    pub cluster: Option<ClusterId>,
    pub images: Vec<ArtifactObservation>,
}

pub(crate) fn git_source(resource: &FluxResource) -> Option<&Reference> {
    source_reference(resource).filter(|reference| reference.kind == GIT_REPOSITORY)
}

fn unit(kind: Kind, namespace: &str, name: &str) -> Option<UnitId> {
    if namespace.is_empty() || name.is_empty() {
        return None;
    }

    UnitId::new(&format!("{kind}/{namespace}/{name}"))
}

fn app_label(resource: &FluxResource) -> Option<String> {
    [LABEL_PART_OF, LABEL_NAME].into_iter().find_map(|key| {
        resource
            .metadata
            .labels
            .get(key)
            .filter(|value| !value.is_empty())
            .cloned()
    })
}

fn path_scopes(resource: &FluxResource) -> Vec<PathScope> {
    let path = match resource.kind {
        Kind::Kustomization => resource.spec.path.as_deref(),
        Kind::HelmRelease => resource
            .spec
            .chart
            .as_ref()
            .map(|chart| chart.spec.chart.as_str())
            .filter(|chart| !chart.is_empty()),
    };

    path.map(PathScope::new).into_iter().collect()
}

fn current_commit(resource: &FluxResource) -> Option<CommitSha> {
    current_revision(resource)
        .map(split_revision)
        .and_then(|(_, commit)| commit)
        .and_then(CommitSha::parse)
}

fn targets(resource: &FluxResource, facts: &Facts) -> Vec<TargetObservation> {
    let namespace = resource.target_namespace();

    facts
        .cluster
        .iter()
        .filter(|_| !namespace.is_empty())
        .map(|cluster| TargetObservation {
            cluster: cluster.clone(),
            namespace: Some(namespace.to_string()),
            name: namespace.to_string(),
            lifetime: TargetLifetime::Standing,
        })
        .collect()
}

pub(crate) fn pipeline(resource: &FluxResource, facts: &Facts) -> Correlation {
    let metadata = &resource.metadata;
    let targets = targets(resource, facts);
    let consumed = if targets.is_empty() {
        Vec::new()
    } else {
        facts
            .images
            .iter()
            .map(|artifact| ConsumedArtifact {
                artifact: artifact.clone(),
                target: Some(0),
            })
            .collect()
    };

    Correlation {
        unit: unit(resource.kind, &metadata.namespace, &metadata.name),
        unit_depends_on: resource
            .spec
            .depends_on
            .iter()
            .filter_map(|dependency| {
                let namespace = dependency
                    .namespace
                    .as_deref()
                    .filter(|namespace| !namespace.is_empty())
                    .unwrap_or(&metadata.namespace);
                unit(resource.kind, namespace, &dependency.name)
            })
            .collect(),
        app_label: app_label(resource),
        sources: facts
            .repo
            .iter()
            .map(|repo| SourceObservation {
                repo: repo.clone(),
                path_scopes: path_scopes(resource),
                commit: current_commit(resource),
            })
            .collect(),
        targets,
        consumed,
        ..Correlation::default()
    }
}

pub(crate) fn attach_runs(runs: &mut [PipelineRun], resource: &FluxResource, facts: &Facts) {
    let Some(repo) = &facts.repo else {
        return;
    };
    let path_scopes = path_scopes(resource);

    for run in runs {
        let Some(commit) = run.commit_sha.as_deref().and_then(CommitSha::parse) else {
            continue;
        };
        run.correlation.sources = vec![SourceObservation {
            repo: repo.clone(),
            path_scopes: path_scopes.clone(),
            commit: Some(commit),
        }];
    }
}

#[cfg(test)]
mod tests;
