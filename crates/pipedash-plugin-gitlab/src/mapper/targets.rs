use std::collections::{
    BTreeSet,
    HashMap,
    HashSet,
};

use pipedash_plugin_api::{
    ClusterId,
    RepoId,
    TargetLifetime,
    TargetObservation,
};

use crate::types;

pub(crate) fn run_targets(
    project: &types::Project, deployments: &[types::Deployment],
    environments: &[types::Environment],
) -> HashMap<i64, Vec<TargetObservation>> {
    let Some(repo) = RepoId::parse(&project.web_url) else {
        return HashMap::new();
    };
    let Some(cluster) = ClusterId::from_host(&format!("{}/{}", repo.host(), repo.path())) else {
        return HashMap::new();
    };
    let short_lived: HashSet<&str> = environments
        .iter()
        .filter(|environment| environment.auto_stop_at.is_some())
        .map(|environment| environment.name.as_str())
        .collect();

    let mut names: HashMap<i64, BTreeSet<&str>> = HashMap::new();
    for deployment in deployments {
        let Some(pipeline) = deployment
            .deployable
            .as_ref()
            .and_then(|deployable| deployable.pipeline.as_ref())
        else {
            continue;
        };
        names
            .entry(pipeline.id)
            .or_default()
            .insert(deployment.environment.name.as_str());
    }

    names
        .into_iter()
        .map(|(pipeline_id, names)| {
            let targets = names
                .into_iter()
                .map(|name| TargetObservation {
                    cluster: cluster.clone(),
                    namespace: None,
                    name: name.to_string(),
                    lifetime: if short_lived.contains(name) {
                        TargetLifetime::ShortLived
                    } else {
                        TargetLifetime::Standing
                    },
                })
                .collect();
            (pipeline_id, targets)
        })
        .collect()
}
