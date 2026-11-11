use std::collections::BTreeMap;

use crate::application::services::correlation_service::state::{
    ApplicationState,
    GroupState,
};
use crate::domain::application::dto::FlowNode;

pub(super) const SOURCE_COLUMN: u32 = 0;
pub(super) const BUILD_COLUMN: u32 = 1;
pub(super) const ARTIFACT_COLUMN: u32 = 2;
pub(super) const FIRST_TARGET_COLUMN: u32 = 3;
pub(super) const MAX_ARTIFACT_NODES: usize = 4;

pub(super) fn target_columns(
    groups: &[String], directions: &[(String, String)],
) -> BTreeMap<String, u32> {
    let mut depth: BTreeMap<&str, u32> = groups.iter().map(|group| (group.as_str(), 0)).collect();
    for _ in 0..groups.len() {
        let mut moved = false;
        for (from, to) in directions {
            let Some(&base) = depth.get(from.as_str()) else {
                continue;
            };
            let Some(slot) = depth.get_mut(to.as_str()) else {
                continue;
            };
            if *slot <= base {
                *slot = base + 1;
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    depth
        .into_iter()
        .map(|(group, depth)| (group.to_string(), FIRST_TARGET_COLUMN + depth))
        .collect()
}

pub(in super::super) fn ordered_groups(state: &ApplicationState) -> Vec<&GroupState> {
    let keys: Vec<String> = state
        .groups
        .iter()
        .filter(|group| !group.short_lived)
        .map(|group| group.key.clone())
        .collect();
    let pairs: Vec<(String, String)> = state
        .directions
        .iter()
        .map(|d| (d.from_group.clone(), d.to_group.clone()))
        .collect();
    let columns = target_columns(&keys, &pairs);
    let mut groups: Vec<&GroupState> = state.groups.iter().collect();
    groups.sort_by_cached_key(|group| {
        (
            group.short_lived,
            columns.get(&group.key).copied().unwrap_or(0),
            group.label.clone(),
            group.key.clone(),
        )
    });
    groups
}

pub(super) fn split_artifacts<T>(mut items: Vec<T>) -> (Vec<T>, usize) {
    if items.len() <= MAX_ARTIFACT_NODES {
        return (items, 0);
    }
    let hidden = items.len() - MAX_ARTIFACT_NODES;
    items.truncate(MAX_ARTIFACT_NODES);
    (items, hidden)
}

fn index(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

pub(super) fn arrange(nodes: &mut [FlowNode]) -> (u32, u32) {
    let mut columns: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (position, node) in nodes.iter().enumerate() {
        columns.entry(node.column).or_default().push(position);
    }
    let mut rows = 0;
    for positions in columns.values_mut() {
        positions.sort_by(|a, b| {
            (&nodes[*a].title, &nodes[*a].id).cmp(&(&nodes[*b].title, &nodes[*b].id))
        });
        for (row, position) in positions.iter().enumerate() {
            nodes[*position].row = index(row);
        }
        rows = rows.max(positions.len());
    }
    let width = columns.keys().next_back().map_or(0, |last| last + 1);
    (width, index(rows))
}

#[cfg(test)]
mod tests;
