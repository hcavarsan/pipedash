use std::collections::{
    BTreeMap,
    BTreeSet,
};

use super::configured::{
    self,
    ConfigApp,
};
use super::stored::{
    confirmed,
    Identity,
    Op,
};
use super::world::{
    Subject,
    World,
};
use crate::domain::application::{
    IdentityKind,
    MemberSource,
    Stage,
};

mod persist;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub(super) struct Explicit {
    pub stage: Option<Stage>,
    pub source: MemberSource,
}

#[derive(Debug, Clone)]
pub(super) struct PlannedApp {
    pub identity: Identity,
    pub name: String,
    pub hidden: bool,
    pub units: BTreeSet<String>,
    pub explicit: BTreeMap<String, Explicit>,
    pub excluded: BTreeSet<String>,
}

#[derive(Debug, Default)]
pub(super) struct Plan {
    pub apps: BTreeMap<String, PlannedApp>,
    pub unit_app: BTreeMap<String, String>,
    pub unit_source: BTreeMap<String, MemberSource>,
    pub rejected: BTreeSet<String>,
    pub confirmed: BTreeMap<String, BTreeSet<String>>,
    pub unlinked: BTreeSet<(String, String)>,
    pub stages: BTreeMap<(String, String), Stage>,
    pub warnings: BTreeMap<String, Vec<String>>,
    pub merged: BTreeMap<String, String>,
    pub displaced: BTreeMap<String, BTreeSet<String>>,
}

fn auto_identity(unit: &str, subject: &Subject) -> Identity {
    match &subject.app_ref {
        Some((key, _)) => Identity::new(IdentityKind::AppRef, key.clone()),
        None => Identity::new(IdentityKind::Unit, unit),
    }
}

fn follow(map: &BTreeMap<String, String>, start: &str) -> String {
    let mut current = start;
    for _ in 0..=map.len() {
        match map.get(current) {
            Some(next) => current = next,
            None => break,
        }
    }
    current.to_string()
}

#[derive(Default)]
struct Draft {
    claims: BTreeMap<String, (Identity, MemberSource)>,
    explicit: BTreeMap<String, BTreeMap<String, Explicit>>,
    created: BTreeMap<String, String>,
    merges: Vec<(String, String)>,
    renames: Vec<(String, String)>,
    hides: Vec<(String, bool)>,
    rejected: BTreeSet<String>,
    unlinked: BTreeSet<(String, String)>,
    stages: Vec<(String, String, Stage)>,
    displaced: Vec<(String, String)>,
}

impl Draft {
    fn assign(
        &mut self, world: &World, pipeline: &str, stage: Option<Stage>, identity: &Identity,
        source: MemberSource,
    ) {
        if let Some(unit) = world.subjects.get(pipeline).and_then(|s| s.unit.clone()) {
            for members in self.explicit.values_mut() {
                members.remove(pipeline);
            }
            self.claims.insert(unit, (identity.clone(), source));
        }
        self.explicit
            .entry(identity.encode())
            .or_default()
            .insert(pipeline.to_string(), Explicit { stage, source });
    }

    fn unlink(&mut self, world: &World, application: &str, pipeline: &str) {
        if let Some(members) = self.explicit.get_mut(application) {
            members.remove(pipeline);
        }
        self.unlinked
            .insert((application.to_string(), pipeline.to_string()));
        let Some(subject) = world.subjects.get(pipeline) else {
            return;
        };
        let Some(unit) = &subject.unit else {
            return;
        };
        let auto = auto_identity(unit, subject);
        let claimed_here = self
            .claims
            .get(unit)
            .is_some_and(|(identity, _)| identity.encode() == application);
        if claimed_here && auto.encode() != application {
            self.claims
                .insert(unit.clone(), (auto, MemberSource::Exact));
        }
    }

    fn apply(&mut self, world: &World, op: &Op) {
        match op {
            Op::Create { key, name, members } => {
                let identity = Identity::new(IdentityKind::User, key.clone());
                self.created.insert(identity.encode(), name.clone());
                for member in members {
                    let source = MemberSource::Override;
                    self.assign(world, &member.pipeline_id, member.stage, &identity, source);
                }
            }
            Op::Split {
                key,
                name,
                pipeline_ids,
            } => {
                let identity = Identity::new(IdentityKind::User, key.clone());
                self.created.insert(identity.encode(), name.clone());
                for id in pipeline_ids {
                    self.assign(world, id, None, &identity, MemberSource::Override);
                }
            }
            Op::Link {
                application,
                pipeline_id,
                stage,
            } => {
                if let Some(identity) = Identity::decode(application) {
                    self.unlinked
                        .remove(&(application.clone(), pipeline_id.clone()));
                    self.assign(
                        world,
                        pipeline_id,
                        *stage,
                        &identity,
                        MemberSource::Override,
                    );
                }
            }
            Op::Unlink {
                application,
                pipeline_id,
            } => self.unlink(world, application, pipeline_id),
            Op::Reject { suggestion_id } => {
                self.rejected.insert(suggestion_id.clone());
            }
            Op::Merge { into, from } => self.merges.push((into.clone(), from.clone())),
            Op::Rename { application, name } => {
                self.renames.push((application.clone(), name.clone()))
            }
            Op::Hide {
                application,
                hidden,
            } => self.hides.push((application.clone(), *hidden)),
            Op::Stage {
                application,
                pipeline_id,
                stage,
            } => self
                .stages
                .push((application.clone(), pipeline_id.clone(), *stage)),
            Op::TargetName { .. }
            | Op::TargetGroup { .. }
            | Op::Direction { .. }
            | Op::Confirm { .. } => {}
        }
    }

    fn apply_config(&mut self, world: &World, config: &[ConfigApp]) {
        for app in config {
            let identity = Identity::new(IdentityKind::Config, app.key.clone());
            let encoded = identity.encode();
            for unit in &app.units {
                let claim = (identity.clone(), MemberSource::Config);
                if let Some((old, _)) = self.claims.insert(unit.clone(), claim) {
                    if old != identity {
                        self.displaced.push((old.encode(), encoded.clone()));
                    }
                }
            }
            let members = self.explicit.entry(encoded.clone()).or_default();
            for (pipeline, stage) in &app.members {
                members.insert(
                    pipeline.clone(),
                    Explicit {
                        stage: *stage,
                        source: MemberSource::Config,
                    },
                );
            }
            for pipeline in app.members.keys() {
                let has_unit = world
                    .subjects
                    .get(pipeline)
                    .is_some_and(|s| s.unit.is_some());
                for (key, other) in self.explicit.iter_mut() {
                    if *key != encoded && has_unit && other.remove(pipeline).is_some() {
                        self.displaced.push((key.clone(), encoded.clone()));
                    }
                }
            }
        }
    }
}

fn entry(apps: &mut BTreeMap<String, PlannedApp>, identity: Identity) -> &mut PlannedApp {
    apps.entry(identity.encode()).or_insert_with(|| PlannedApp {
        identity,
        name: String::new(),
        hidden: false,
        units: BTreeSet::new(),
        explicit: BTreeMap::new(),
        excluded: BTreeSet::new(),
    })
}

fn decode(encoded: &str, fallback: &Identity) -> Identity {
    Identity::decode(encoded).unwrap_or_else(|| fallback.clone())
}

fn display_name(
    world: &World, draft: &Draft, merged: &BTreeMap<String, String>, config: &[ConfigApp],
    app: &PlannedApp,
) -> String {
    let encoded = app.identity.encode();
    if let Some((_, name)) = draft
        .renames
        .iter()
        .rev()
        .find(|(target, _)| follow(merged, target) == encoded)
    {
        return name.clone();
    }
    if let Some(found) = config.iter().find(|c| c.key == app.identity.key) {
        if app.identity.kind == IdentityKind::Config {
            return found.name.clone();
        }
    }
    if let Some(name) = draft.created.get(&encoded) {
        return name.clone();
    }
    let own = match app.identity.kind {
        IdentityKind::Unit => world
            .units
            .get(&app.identity.key)
            .filter(|_| app.units.contains(&app.identity.key))
            .map(|id| world.name_of(id)),
        IdentityKind::AppRef => app
            .units
            .iter()
            .filter_map(|u| world.unit_subjects().find(|(k, _)| *k == u))
            .find_map(|(_, s)| s.app_ref.as_ref().map(|(_, id)| id.clone())),
        _ => None,
    };
    own.or_else(|| world.applications.get(&encoded).map(|r| r.name.clone()))
        .unwrap_or_else(|| app.identity.key.clone())
}

pub(super) fn plan(world: &World) -> Plan {
    let mut draft = Draft::default();
    for (unit, subject) in world.unit_subjects() {
        draft.claims.insert(
            unit.clone(),
            (auto_identity(unit, subject), MemberSource::Exact),
        );
    }
    for stored in &world.overrides {
        draft.apply(world, &stored.op);
    }
    let config = configured::plan(world);
    draft.apply_config(world, &config);

    let mut merged: BTreeMap<String, String> = BTreeMap::new();
    for (into, from) in &draft.merges {
        merged.insert(from.clone(), into.clone());
    }
    let mut plan = Plan {
        rejected: draft.rejected.clone(),
        confirmed: confirmed(&world.overrides),
        unlinked: draft
            .unlinked
            .iter()
            .map(|(app, pipeline)| (follow(&merged, app), pipeline.clone()))
            .collect(),
        ..Plan::default()
    };
    for (unit, (identity, source)) in &draft.claims {
        let target = decode(&follow(&merged, &identity.encode()), identity);
        let encoded = target.encode();
        entry(&mut plan.apps, target).units.insert(unit.clone());
        plan.unit_app.insert(unit.clone(), encoded);
        plan.unit_source.insert(unit.clone(), *source);
    }
    for (encoded, members) in &draft.explicit {
        let identity = decode(
            &follow(&merged, encoded),
            &Identity::new(IdentityKind::User, ""),
        );
        entry(&mut plan.apps, identity)
            .explicit
            .extend(members.clone());
    }
    for encoded in draft.created.keys() {
        let identity = decode(
            &follow(&merged, encoded),
            &Identity::new(IdentityKind::User, ""),
        );
        entry(&mut plan.apps, identity);
    }
    for app in &config {
        let identity = Identity::new(IdentityKind::Config, app.key.clone());
        let encoded = identity.encode();
        let planned = entry(&mut plan.apps, identity);
        planned.excluded = app.excluded.clone();
        planned.hidden = app.hidden;
        plan.warnings.insert(encoded, app.warnings.clone());
    }
    plan.apps.retain(|_, app| {
        app.identity.kind == IdentityKind::Config
            || !app.units.is_empty()
            || !app.explicit.is_empty()
    });
    for (from, into) in &draft.displaced {
        plan.displaced
            .entry(follow(&merged, from))
            .or_default()
            .insert(into.clone());
    }
    for (application, pipeline, stage) in &draft.stages {
        plan.stages
            .insert((follow(&merged, application), pipeline.clone()), *stage);
    }
    let names: Vec<(String, String)> = plan
        .apps
        .values()
        .map(|app| {
            (
                app.identity.encode(),
                display_name(world, &draft, &merged, &config, app),
            )
        })
        .collect();
    for (encoded, name) in names {
        let hidden = draft
            .hides
            .iter()
            .rev()
            .find(|(target, _)| follow(&merged, target) == encoded)
            .map(|(_, hidden)| *hidden);
        if let Some(app) = plan.apps.get_mut(&encoded) {
            app.name = name;
            if app.identity.kind != IdentityKind::Config {
                app.hidden = hidden.unwrap_or(false);
            }
        }
    }
    plan.merged = merged;
    plan
}

impl Plan {
    pub(super) fn final_identity(&self, encoded: &str) -> String {
        follow(&self.merged, encoded)
    }
}
