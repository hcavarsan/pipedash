use pipedash_plugin_api::{
    ClusterId,
    CommitSha,
    ConsumedArtifact,
    Correlation,
    ImageRef,
    RepoId,
    TargetLifetime,
    TargetObservation,
    UnitId,
};

use super::{
    is_docker_image,
    nonempty,
    repository,
    Placement,
};
use crate::types::{
    Application,
    Deployment,
};

fn repo(app: &Application) -> Option<RepoId> {
    if is_docker_image(app) {
        return None;
    }
    nonempty(app.git_repository.as_deref()).and_then(RepoId::parse)
}

fn image(app: &Application) -> Option<ImageRef> {
    if !is_docker_image(app) {
        return None;
    }
    ImageRef::parse(&repository(app))
}

fn target(app: &Application, placement: Option<&Placement>) -> Option<TargetObservation> {
    let server = app.server_name()?;
    let name = placement
        .map(|placement| placement.environment.as_str())
        .filter(|environment| !environment.is_empty())
        .unwrap_or(server);
    Some(TargetObservation {
        cluster: ClusterId::from_host(server)?,
        namespace: None,
        name: name.to_string(),
        lifetime: TargetLifetime::Standing,
    })
}

pub(super) fn pipeline(app: &Application, placement: Option<&Placement>) -> Correlation {
    let targets: Vec<TargetObservation> = target(app, placement).into_iter().collect();
    let bound = (!targets.is_empty()).then_some(0);
    Correlation {
        unit: UnitId::new(&app.uuid),
        targets,
        consumed: image(app)
            .map(|image| ConsumedArtifact {
                artifact: image.into_observation(),
                target: bound,
            })
            .into_iter()
            .collect(),
        ..Correlation::single_source(repo(app), None)
    }
}

pub(super) fn run(app: Option<&Application>, deployment: &Deployment) -> Correlation {
    Correlation::single_source(
        app.and_then(repo),
        deployment.commit.as_deref().and_then(CommitSha::parse),
    )
}
