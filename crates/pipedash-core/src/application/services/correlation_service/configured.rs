use std::collections::{
    BTreeMap,
    BTreeSet,
};

use super::copy;
use super::world::World;
use crate::domain::application::Stage;
use crate::infrastructure::config::schema::applications::{
    glob_matches,
    PipelineSelector,
};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Default)]
pub(super) struct ConfigApp {
    pub key: String,
    pub name: String,
    pub hidden: bool,
    pub units: BTreeSet<String>,
    pub members: BTreeMap<String, Option<Stage>>,
    pub excluded: BTreeSet<String>,
    pub warnings: Vec<String>,
}

fn matching(world: &World, provider: &str, pattern: &str) -> BTreeSet<String> {
    world
        .pipelines
        .keys()
        .filter(|id| world.provider_key(id) == Some(provider))
        .filter(|id| {
            world
                .local_id(id)
                .is_some_and(|local| glob_matches(pattern, local))
        })
        .cloned()
        .collect()
}

fn selected(world: &World, selectors: &[PipelineSelector]) -> BTreeSet<String> {
    selectors
        .iter()
        .flat_map(|s| matching(world, &s.provider, &s.pipeline))
        .collect()
}

pub(super) fn plan(world: &World) -> Vec<ConfigApp> {
    let mut apps = Vec::new();
    let mut claimed: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (key, config) in &world.config.applications {
        let mut app = ConfigApp {
            key: key.clone(),
            name: config.name.clone(),
            hidden: config.hidden,
            ..ConfigApp::default()
        };
        let excluded = selected(world, &config.exclude);
        for member in &config.members {
            let found = matching(world, &member.provider, &member.pipeline);
            if found.is_empty() {
                app.warnings.push(copy::selector_matches_nothing(
                    &member.provider,
                    &member.pipeline,
                ));
            }
            for id in found.difference(&excluded) {
                app.members.insert(id.clone(), member.stage);
            }
        }
        for id in app.members.keys() {
            claimed.entry(id.clone()).or_default().push(apps.len());
        }
        app.excluded = excluded;
        apps.push(app);
    }
    for (id, owners) in claimed.iter().filter(|(_, owners)| owners.len() > 1) {
        let local = world.local_id(id).unwrap_or(id).to_string();
        let keys: Vec<String> = owners.iter().map(|i| apps[*i].key.clone()).collect();
        for owner in owners {
            let app = &mut apps[*owner];
            app.members.remove(id);
            for other in keys.iter().filter(|k| **k != app.key) {
                app.warnings.push(copy::config_conflict(&local, other));
            }
        }
    }
    for app in &mut apps {
        let units: Vec<String> = app
            .members
            .keys()
            .filter_map(|id| world.subjects.get(id).and_then(|s| s.unit.clone()))
            .collect();
        app.units.extend(units);
    }
    apps
}
