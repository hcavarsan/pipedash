use std::collections::BTreeSet;

use crate::application::services::correlation_service::applications::Plan;
use crate::application::services::correlation_service::configured;
use crate::application::services::correlation_service::copy::describe;
use crate::application::services::correlation_service::stored::{
    Identity,
    Op,
    StoredOverride,
};
use crate::application::services::correlation_service::world::World;
use crate::domain::application::overrides::OverrideEntry;
use crate::domain::application::IdentityKind;
use crate::infrastructure::database::ApplicationRow;

pub(in super::super) struct ConfigClaims {
    pipelines: BTreeSet<String>,
    units: BTreeSet<String>,
}

impl ConfigClaims {
    pub(in super::super) fn of(world: &World) -> Self {
        let mut claims = Self {
            pipelines: BTreeSet::new(),
            units: BTreeSet::new(),
        };
        for app in configured::plan(world) {
            claims.pipelines.extend(app.members.into_keys());
            claims.units.extend(app.units);
        }
        claims
    }
}

pub(in super::super) fn is_shadowed(world: &World, config: &ConfigClaims, op: &Op) -> bool {
    if op.pipelines().iter().any(|p| config.pipelines.contains(*p)) {
        return true;
    }
    let Some(identity) = op.application().and_then(Identity::decode) else {
        return false;
    };
    match identity.kind {
        IdentityKind::Unit => config.units.contains(&identity.key),
        IdentityKind::AppRef => world.unit_subjects().any(|(unit, subject)| {
            config.units.contains(unit)
                && subject
                    .app_ref
                    .as_ref()
                    .is_some_and(|(key, _)| *key == identity.key)
        }),
        IdentityKind::User | IdentityKind::Config => false,
    }
}

fn describe(world: &World, op: &Op) -> String {
    match op {
        Op::Link { pipeline_id, .. } => describe::link(&world.name_of(pipeline_id)),
        Op::Unlink { pipeline_id, .. } => describe::unlink(&world.name_of(pipeline_id)),
        Op::Reject { .. } => describe::reject(),
        Op::Confirm { repo, commit, .. } => describe::confirm(repo, commit),
        Op::Create { name, .. } => describe::create(name),
        Op::Merge { from, .. } => describe::merge(&label_of(world, from)),
        Op::Split { name, .. } => describe::split(name),
        Op::Rename { name, .. } => describe::rename(name),
        Op::Hide { hidden, .. } => describe::hide(*hidden),
        Op::Stage {
            pipeline_id, stage, ..
        } => describe::stage(&world.name_of(pipeline_id), *stage),
        Op::TargetName { name, .. } => describe::target_name(name),
        Op::TargetGroup { group_name, .. } => describe::target_group(group_name.as_deref()),
        Op::Direction {
            from_group,
            to_group,
            present,
            ..
        } => describe::direction(from_group, to_group, *present),
    }
}

fn label_of(world: &World, encoded: &str) -> String {
    world
        .applications
        .get(encoded)
        .map_or_else(|| encoded.to_string(), |row| row.name.clone())
}

pub(in super::super) fn override_entries(
    world: &World, plan: &Plan, config: &ConfigClaims, application: &ApplicationRow,
) -> Vec<OverrideEntry> {
    let Some(identity) = Identity::of_row(application) else {
        return Vec::new();
    };
    let encoded = identity.encode();
    world
        .overrides
        .iter()
        .filter_map(|stored| {
            let displaced = displaced_into(plan, stored, &encoded);
            if !displaced && !belongs(world, plan, stored, application.id, &identity, &encoded) {
                return None;
            }
            Some(OverrideEntry {
                id: stored.id,
                kind: stored.op.kind(),
                description: describe(world, &stored.op),
                author: stored.author.clone(),
                created_at: stored.created_at,
                shadowed: displaced || is_shadowed(world, config, &stored.op),
            })
        })
        .collect()
}

fn displaced_into(plan: &Plan, stored: &StoredOverride, encoded: &str) -> bool {
    let owner = stored.op.application().map(str::to_string).or_else(|| {
        stored
            .op
            .owned_key()
            .map(|key| Identity::new(IdentityKind::User, key).encode())
    });
    let Some(owner) = owner else {
        return false;
    };
    let owner = plan.final_identity(&owner);
    !plan.apps.contains_key(&owner)
        && plan
            .displaced
            .get(&owner)
            .is_some_and(|into| into.contains(encoded))
}

fn consumes(world: &World, id: i64, digest: &str) -> bool {
    world.members.get(&id).into_iter().flatten().any(|member| {
        world
            .subjects
            .get(&member.pipeline_id)
            .is_some_and(|subject| {
                subject.unit.is_some() && subject.consumed_digests().contains(digest)
            })
    })
}

fn belongs(
    world: &World, plan: &Plan, stored: &StoredOverride, id: i64, identity: &Identity,
    encoded: &str,
) -> bool {
    stored.application_id == Some(id)
        || stored.op.owned_key() == Some(identity.key.as_str())
        || stored
            .op
            .application()
            .is_some_and(|app| plan.final_identity(app) == encoded)
        || matches!(&stored.op, Op::Confirm { digest, .. } if consumes(world, id, digest))
}
