use std::collections::{
    BTreeMap,
    BTreeSet,
};

use super::applications::{
    Plan,
    PlannedApp,
};
use super::copy;
use super::stored::Op;
use super::vocab::{
    LIFETIME_SHORT_LIVED,
    LIFETIME_STANDING,
};
use super::world::{
    Subject,
    TargetRef,
    World,
};
use crate::domain::application::IdentityKind;
use crate::infrastructure::config::schema::applications::TargetFileConfig;
use crate::infrastructure::database::{
    ApplicationTargetRow,
    TargetDirectionRow,
};

#[cfg(test)]
mod tests;

pub(super) const SHORT_LIVED_GROUP: &str = "short_lived";
const SOURCE_PROVIDER: &str = "provider";
const SOURCE_OVERRIDE: &str = "override";
const SOURCE_CONFIG: &str = "config";

pub(super) struct Targets {
    pub rows: Vec<ApplicationTargetRow>,
    pub directions: Vec<TargetDirectionRow>,
}

type Directions = BTreeMap<(String, String), &'static str>;

pub(super) fn slug(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect()
}

pub(super) fn user_group(name: &str) -> String {
    format!("user:{}", slug(name))
}

pub(super) fn config_group(table: &str) -> String {
    format!("config:{table}")
}

pub(super) fn cycles(directions: &Directions, from: &str, to: &str) -> bool {
    if from == to {
        return true;
    }
    let mut stack = vec![to];
    let mut seen = BTreeSet::new();
    while let Some(node) = stack.pop() {
        if node == from {
            return true;
        }
        if !seen.insert(node) {
            continue;
        }
        stack.extend(
            directions
                .keys()
                .filter(|(origin, _)| origin == node)
                .map(|(_, next)| next.as_str()),
        );
    }
    false
}

fn default_group(subject: &Subject, target: &TargetRef) -> String {
    if target.short_lived {
        return SHORT_LIVED_GROUP.to_string();
    }
    match &subject.parent {
        Some(parent) => format!("parent:{parent}"),
        None => target.key(),
    }
}

fn row_matches(table: &TargetFileConfig, row: &ApplicationTargetRow) -> bool {
    let (cluster, namespace) = row
        .target_key
        .split_once('|')
        .unwrap_or((&row.target_key, ""));
    table.matches(cluster, namespace, &row.name)
}

fn base_rows(world: &World, app: &PlannedApp) -> BTreeMap<String, ApplicationTargetRow> {
    let mut rows = BTreeMap::new();
    for unit in &app.units {
        let Some(subject) = world.units.get(unit).and_then(|id| world.subjects.get(id)) else {
            continue;
        };
        for target in &subject.targets {
            rows.entry(target.key())
                .or_insert_with(|| ApplicationTargetRow {
                    application_id: 0,
                    target_key: target.key(),
                    name: target.name.clone(),
                    group_key: default_group(subject, target),
                    lifetime: if target.short_lived {
                        LIFETIME_SHORT_LIVED
                    } else {
                        LIFETIME_STANDING
                    }
                    .to_string(),
                });
        }
    }
    rows
}

fn apply_config_groups(
    world: &World, app: &PlannedApp, rows: &mut BTreeMap<String, ApplicationTargetRow>,
) {
    let Some(config) = world.config.applications.get(&app.identity.key) else {
        return;
    };
    for (table_key, table) in config.targets.iter().filter(|(_, t)| t.group.is_none()) {
        let tables: Vec<&TargetFileConfig> = config
            .targets
            .iter()
            .filter(|(key, t)| *key == table_key || t.group.as_deref() == Some(table_key))
            .map(|(_, t)| t)
            .collect();
        let matched: Vec<String> = rows
            .values()
            .filter(|row| {
                !row.group_key.starts_with("config:") && tables.iter().any(|t| row_matches(t, row))
            })
            .map(|row| row.target_key.clone())
            .collect();
        let single = matched.len() == 1;
        for key in matched {
            if let Some(row) = rows.get_mut(&key) {
                row.group_key = config_group(table_key);
                if single {
                    row.name = table.name.clone();
                }
            }
        }
    }
}

fn apply_user_groups(
    world: &World, plan: &Plan, app: &PlannedApp, rows: &mut BTreeMap<String, ApplicationTargetRow>,
) {
    let encoded = app.identity.encode();
    for stored in &world.overrides {
        let Op::TargetGroup {
            application,
            target_keys,
            group_name,
        } = &stored.op
        else {
            continue;
        };
        if plan.final_identity(application) != encoded {
            continue;
        }
        for key in target_keys {
            let Some(row) = rows.get_mut(key) else {
                continue;
            };
            row.group_key = match group_name {
                Some(name) => user_group(name),
                None => key.clone(),
            };
        }
    }
}

fn groups_of(rows: &BTreeMap<String, ApplicationTargetRow>, subject: &Subject) -> BTreeSet<String> {
    subject
        .targets
        .iter()
        .filter_map(|t| rows.get(&t.key()))
        .map(|row| row.group_key.clone())
        .collect()
}

fn provider_directions(
    world: &World, app: &PlannedApp, rows: &BTreeMap<String, ApplicationTargetRow>,
    directions: &mut Directions,
) {
    for unit in &app.units {
        let Some(subject) = world.units.get(unit).and_then(|id| world.subjects.get(id)) else {
            continue;
        };
        for dependency in subject.depends_on.iter().filter(|d| app.units.contains(*d)) {
            let Some(upstream) = world
                .units
                .get(dependency)
                .and_then(|id| world.subjects.get(id))
            else {
                continue;
            };
            for from in groups_of(rows, upstream) {
                for to in groups_of(rows, subject) {
                    if !cycles(directions, &from, &to) {
                        directions.insert((from.clone(), to), SOURCE_PROVIDER);
                    }
                }
            }
        }
    }
}

fn config_directions(
    world: &World, app: &PlannedApp, groups: &BTreeSet<String>, directions: &mut Directions,
) {
    let Some(config) = world.config.applications.get(&app.identity.key) else {
        return;
    };
    for (table_key, table) in config.targets.iter().filter(|(_, t)| t.group.is_none()) {
        let to = config_group(table_key);
        for before in &table.after {
            let owner = config
                .targets
                .get(before)
                .map_or(before.as_str(), |t| t.owner(before));
            let from = config_group(owner);
            let known = groups.contains(&from) && groups.contains(&to);
            if known && !cycles(directions, &from, &to) {
                directions.insert((from, to.clone()), SOURCE_CONFIG);
            }
        }
    }
}

fn user_directions(
    world: &World, plan: &Plan, app: &PlannedApp, groups: &BTreeSet<String>,
    directions: &mut Directions,
) {
    let encoded = app.identity.encode();
    for stored in &world.overrides {
        let Op::Direction {
            application,
            from_group,
            to_group,
            present,
        } = &stored.op
        else {
            continue;
        };
        if plan.final_identity(application) != encoded {
            continue;
        }
        let pair = (from_group.clone(), to_group.clone());
        if !present {
            directions.remove(&pair);
        } else if groups.contains(from_group)
            && groups.contains(to_group)
            && !cycles(directions, from_group, to_group)
        {
            directions.insert(pair, SOURCE_OVERRIDE);
        }
    }
}

pub(super) fn compute(world: &World, plan: &Plan, app: &PlannedApp) -> Targets {
    let mut rows = base_rows(world, app);
    if app.identity.kind == IdentityKind::Config {
        apply_config_groups(world, app, &mut rows);
    } else {
        apply_user_groups(world, plan, app, &mut rows);
    }
    let groups: BTreeSet<String> = rows.values().map(|r| r.group_key.clone()).collect();
    let mut directions = Directions::new();
    provider_directions(world, app, &rows, &mut directions);
    if app.identity.kind == IdentityKind::Config {
        config_directions(world, app, &groups, &mut directions);
    } else {
        user_directions(world, plan, app, &groups, &mut directions);
    }
    Targets {
        rows: rows.into_values().collect(),
        directions: directions
            .into_iter()
            .map(|((from_group, to_group), source)| TargetDirectionRow {
                application_id: 0,
                from_group,
                to_group,
                source: source.to_string(),
            })
            .collect(),
    }
}

pub(super) fn group_labels(
    world: &World, plan: &Plan, app: &PlannedApp, rows: &[ApplicationTargetRow],
) -> BTreeMap<String, String> {
    let encoded = app.identity.encode();
    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    let mut members: BTreeMap<&str, Vec<&ApplicationTargetRow>> = BTreeMap::new();
    for row in rows {
        members.entry(row.group_key.as_str()).or_default().push(row);
    }
    let tables = world
        .config
        .applications
        .get(&app.identity.key)
        .map(|c| &c.targets);
    for (group, targets) in members {
        let label = if let Some(table) = group
            .strip_prefix("config:")
            .and_then(|key| tables.and_then(|t| t.get(key)))
        {
            table.name.clone()
        } else if group == SHORT_LIVED_GROUP {
            copy::short_lived(targets.len())
        } else if let [single] = targets.as_slice() {
            single.name.clone()
        } else if let Some(parent) = group.strip_prefix("parent:") {
            parent
                .split_once(':')
                .map_or(parent, |(_, local)| local)
                .to_string()
        } else {
            group.to_string()
        };
        labels.insert(group.to_string(), label);
    }
    for stored in &world.overrides {
        match &stored.op {
            Op::TargetGroup {
                application,
                group_name: Some(name),
                ..
            } if plan.final_identity(application) == encoded => {
                labels.insert(user_group(name), name.clone());
            }
            Op::TargetName {
                application,
                group_key,
                name,
            } if plan.final_identity(application) == encoded => {
                labels.insert(group_key.clone(), name.clone());
            }
            _ => {}
        }
    }
    labels
}
