use super::PipelineViewService;
use crate::domain::{
    DomainError,
    DomainResult,
    PipelineView,
    ViewKind,
    ViewRef,
};
use crate::infrastructure::database::StoredView;

const CONFIG_READ_ONLY: &str =
    "Views from config.toml can't be changed here. Edit config.toml instead.";

pub(super) fn denied(name: &str, verb: &str) -> DomainError {
    DomainError::InvalidConfig(format!("{name} can't be {verb}"))
}

pub(super) fn checked_name(raw: &str, subject: &str) -> DomainResult<String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(DomainError::InvalidConfig(format!(
            "{subject} name can't be empty"
        )));
    }
    if name.chars().count() > 60 {
        return Err(DomainError::InvalidConfig(format!(
            "{subject} name can't be longer than 60 characters"
        )));
    }
    Ok(name.to_string())
}

pub(super) const MAX_DEPTH: usize = 3;

type Nodes = [(i64, Option<i64>)];

pub(super) fn view_nodes(stored: &[StoredView]) -> Vec<(i64, Option<i64>)> {
    stored
        .iter()
        .filter(|v| v.kind == ViewKind::Custom)
        .map(|v| (v.id, v.parent_id))
        .collect()
}

pub(super) fn custom_nodes(views: &[PipelineView]) -> Vec<(i64, Option<i64>)> {
    views
        .iter()
        .filter_map(|v| match (&v.id, &v.parent) {
            (ViewRef::Custom(id), Some(ViewRef::Custom(parent))) => Some((*id, Some(*parent))),
            (ViewRef::Custom(id), _) => Some((*id, None)),
            _ => None,
        })
        .collect()
}

pub(super) fn lineage(nodes: &Nodes, id: i64) -> Vec<i64> {
    let mut chain = vec![id];
    let mut current = id;
    while chain.len() <= nodes.len() {
        let Some(parent) = nodes.iter().find(|n| n.0 == current).and_then(|n| n.1) else {
            break;
        };
        chain.push(parent);
        current = parent;
    }
    chain
}

fn height(nodes: &Nodes, id: i64) -> usize {
    1 + nodes
        .iter()
        .filter(|n| n.1 == Some(id))
        .map(|n| height(nodes, n.0))
        .max()
        .unwrap_or(0)
}

pub(super) fn subtree(nodes: &Nodes, id: i64) -> Vec<i64> {
    nodes
        .iter()
        .filter(|n| n.1 == Some(id))
        .flat_map(|n| std::iter::once(n.0).chain(subtree(nodes, n.0)))
        .collect()
}

pub(super) fn check_nesting(
    nodes: &Nodes, moving: Option<i64>, parent: i64, subject: &str,
) -> DomainResult<()> {
    let chain = lineage(nodes, parent);
    if moving.is_some_and(|moving| chain.contains(&moving)) {
        return Err(DomainError::InvalidConfig(format!(
            "{subject} can't be moved inside itself"
        )));
    }
    if chain.len() + moving.map_or(1, |moving| height(nodes, moving)) > MAX_DEPTH {
        return Err(DomainError::InvalidConfig(format!(
            "{subject}s can be nested at most {MAX_DEPTH} levels deep"
        )));
    }
    Ok(())
}

pub(super) fn preorder<'a>(sorted: &[&'a StoredView]) -> Vec<&'a StoredView> {
    fn walk<'a>(parent: Option<i64>, sorted: &[&'a StoredView], out: &mut Vec<&'a StoredView>) {
        for view in sorted.iter().filter(|v| v.parent_id == parent) {
            out.push(view);
            walk(Some(view.id), sorted, out);
        }
    }
    let mut out = Vec::with_capacity(sorted.len());
    walk(None, sorted, &mut out);
    out
}

impl PipelineViewService {
    pub(super) fn stored<'a>(
        stored: &'a [StoredView], id: &ViewRef,
    ) -> DomainResult<&'a StoredView> {
        let found = match id {
            ViewRef::All => stored.iter().find(|g| g.kind == ViewKind::All),
            ViewRef::Pinned => stored.iter().find(|g| g.kind == ViewKind::Pinned),
            ViewRef::Custom(n) => stored
                .iter()
                .find(|g| g.id == *n && g.kind == ViewKind::Custom),
            ViewRef::Config(_) => {
                return Err(DomainError::InvalidConfig(CONFIG_READ_ONLY.to_string()))
            }
        };
        found.ok_or_else(|| DomainError::NotFound(format!("Pipeline view {id} not found")))
    }

    pub(super) fn unique_name(
        stored: &[StoredView], name: &str, except: Option<i64>,
    ) -> DomainResult<()> {
        let lower = name.to_lowercase();
        if stored.iter().any(|g| {
            g.kind == ViewKind::Custom && Some(g.id) != except && g.name.to_lowercase() == lower
        }) {
            return Err(DomainError::InvalidConfig(format!(
                "A view named \"{name}\" already exists"
            )));
        }
        Ok(())
    }

    pub(super) fn nest_target<'a>(
        stored: &'a [StoredView], parent: &ViewRef, moving: Option<i64>,
    ) -> DomainResult<&'a StoredView> {
        let target = Self::stored(stored, parent)?;
        if target.kind != ViewKind::Custom {
            return Err(DomainError::InvalidConfig(format!(
                "{} can't contain sub-views",
                target.name
            )));
        }
        check_nesting(&view_nodes(stored), moving, target.id, "View")?;
        Ok(target)
    }
}
