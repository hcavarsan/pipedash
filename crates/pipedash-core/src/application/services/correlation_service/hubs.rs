use std::collections::{
    BTreeMap,
    BTreeSet,
};

use super::world::{
    Subject,
    World,
};
use super::HUB_UNIT_LIMIT;

#[cfg(test)]
mod tests;

pub(super) fn repo_entity(repo: &str) -> String {
    format!("repo:{repo}")
}

pub(super) fn digest_entity(digest: &str) -> String {
    format!("digest:{digest}")
}

pub(super) fn artifact_repository_entity(repository: &str) -> String {
    format!("artifact_repository:{repository}")
}

pub(super) fn label_entity(label: &str) -> String {
    format!("label:{label}")
}

pub(super) fn name_entity(name: &str) -> String {
    format!("name:{name}")
}

fn entities(world: &World, subject: &Subject) -> BTreeSet<String> {
    let mut found = world.keys(subject);
    for artifact in &subject.consumed {
        found.insert(artifact_repository_entity(&artifact.repository));
    }
    for target in &subject.targets {
        found.insert(format!("cluster:{}", target.cluster));
        found.insert(format!("target:{}", target.key()));
    }
    found
}

pub(super) struct Hubs {
    hubs: BTreeSet<String>,
}

impl Hubs {
    pub(super) fn of(world: &World) -> Self {
        let mut counts: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
        for (unit, subject) in world.unit_subjects() {
            for entity in entities(world, subject) {
                counts.entry(entity).or_default().insert(unit);
            }
        }
        Self {
            hubs: counts
                .into_iter()
                .filter(|(_, units)| units.len() > HUB_UNIT_LIMIT)
                .map(|(entity, _)| entity)
                .collect(),
        }
    }

    pub(super) fn is_hub(&self, entity: &str) -> bool {
        self.hubs.contains(entity)
    }
}
