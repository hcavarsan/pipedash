use pipedash_plugin_api::{
    ArtifactObservation,
    ClusterId,
    CommitSha,
    ConsumedArtifact,
    Correlation,
    Digest,
    ImageRef,
    PathScope,
    RepoId,
    SourceObservation,
    TargetLifetime,
    TargetObservation,
    UnitId,
};

use crate::cluster::pods::in_cluster;
use crate::types::logs::Pod;
use crate::types::{
    Application,
    ApplicationDestination,
    ApplicationMetadata,
    ApplicationSource,
    RevisionHistory,
};

const PART_OF_LABEL: &str = "app.kubernetes.io/part-of";
const NAME_LABEL: &str = "app.kubernetes.io/name";
const APPLICATION_SET: &str = "ApplicationSet";

fn sources(
    list: &[ApplicationSource], revisions: &[String], single: Option<&str>,
) -> Vec<SourceObservation> {
    list.iter()
        .enumerate()
        .filter(|(_, source)| source.chart.is_none())
        .filter_map(|(index, source)| {
            let revision = revisions
                .get(index)
                .map(String::as_str)
                .or(single.filter(|_| list.len() == 1));
            Some(SourceObservation {
                repo: RepoId::parse(&source.repo_url)?,
                path_scopes: source
                    .path
                    .as_deref()
                    .map(PathScope::new)
                    .into_iter()
                    .collect(),
                commit: revision.and_then(CommitSha::parse),
            })
        })
        .collect()
}

fn unit(metadata: &ApplicationMetadata) -> Option<UnitId> {
    let namespace = metadata.namespace.as_deref().unwrap_or_default();
    UnitId::new(&format!("{namespace}/{}", metadata.name))
}

fn unit_parent(metadata: &ApplicationMetadata) -> Option<UnitId> {
    let owner = metadata
        .owner_references
        .iter()
        .find(|owner| owner.kind == APPLICATION_SET)?;
    UnitId::new(&format!("appset/{}", owner.name))
}

fn app_label(metadata: &ApplicationMetadata) -> Option<String> {
    [PART_OF_LABEL, NAME_LABEL].into_iter().find_map(|key| {
        let value = metadata.labels.get(key)?.trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn cluster(destination: &ApplicationDestination, uid: Option<&str>) -> Option<ClusterId> {
    if in_cluster(destination) {
        return uid.and_then(ClusterId::from_uid);
    }
    match &destination.server {
        Some(server) => ClusterId::from_server(server),
        None => destination.name.as_deref().and_then(ClusterId::from_host),
    }
}

fn target(destination: &ApplicationDestination, uid: Option<&str>) -> Option<TargetObservation> {
    let namespace = destination
        .namespace
        .clone()
        .filter(|value| !value.is_empty());
    let label = destination.cluster();
    Some(TargetObservation {
        cluster: cluster(destination, uid)?,
        name: match &namespace {
            Some(namespace) => format!("{label}/{namespace}"),
            None => label.to_string(),
        },
        namespace,
        lifetime: TargetLifetime::Standing,
    })
}

fn artifact(image: &str, image_id: Option<&str>) -> Option<ArtifactObservation> {
    let ImageRef {
        repository,
        tag,
        digest,
    } = ImageRef::parse(image)?;
    Some(ArtifactObservation {
        repository,
        tag,
        digest: image_id.and_then(Digest::from_image_id).or(digest),
    })
}

fn consumed(
    app: &Application, pods: Option<&[Pod]>, target: Option<usize>,
) -> Vec<ConsumedArtifact> {
    let mut artifacts: Vec<ArtifactObservation> = match pods {
        Some(pods) => pods
            .iter()
            .flat_map(|pod| &pod.status.container_statuses)
            .filter_map(|status| artifact(status.image.as_deref()?, status.image_id.as_deref()))
            .collect(),
        None => app
            .status
            .summary
            .iter()
            .flat_map(|summary| &summary.images)
            .filter_map(|image| artifact(image, None))
            .collect(),
    };
    artifacts.sort_by(|a, b| {
        (&a.repository, &a.digest, &a.tag).cmp(&(&b.repository, &b.digest, &b.tag))
    });
    artifacts.dedup();

    artifacts
        .into_iter()
        .map(|artifact| ConsumedArtifact { artifact, target })
        .collect()
}

pub(crate) fn pipeline(
    app: &Application, cluster_uid: Option<&str>, pods: Option<&[Pod]>,
) -> Correlation {
    let sync = &app.status.sync;
    let target = target(&app.spec.destination, cluster_uid);

    Correlation {
        unit: unit(&app.metadata),
        unit_parent: unit_parent(&app.metadata),
        app_label: app_label(&app.metadata),
        sources: sources(
            app.spec.source_list(),
            &sync.revisions,
            sync.revision.as_deref(),
        ),
        consumed: consumed(app, pods, target.as_ref().map(|_| 0)),
        targets: target.into_iter().collect(),
        ..Correlation::default()
    }
}

pub(crate) fn history(entry: &RevisionHistory) -> Correlation {
    Correlation {
        sources: sources(
            entry.source_list(),
            &entry.revisions,
            entry.revision.as_deref(),
        ),
        ..Correlation::default()
    }
}

#[cfg(test)]
mod tests;
