use std::collections::{
    BTreeMap,
    HashMap,
    HashSet,
};
use std::sync::Arc;

use tokio::time::Instant;

use super::{
    Built,
    LogService,
    TreeKey,
    COMPLETE_TREE_TTL,
    OPEN_TREE_TTL,
};
use crate::application::services::provider_service::ProviderLease;
use crate::domain::{
    DomainResult,
    LogAvailability,
    LogNode,
    LogNodeKind,
    PipelineRun,
    PipelineStatus,
    RunLogGroup,
    RunLogNode,
    RunLogTree,
};
use crate::infrastructure::deduplication::hash_request;
use crate::infrastructure::run_logs::{
    CacheEpoch,
    StoredTree,
};

impl LogService {
    pub async fn run_log_tree(
        self: &Arc<Self>, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<RunLogTree> {
        let (lease, epoch) = self.lease_of(pipeline_id).await?;
        let built = self.tree_of(&lease, epoch, pipeline_id, run_number).await?;
        if !built.0.complete {
            let run = (pipeline_id.to_string(), run_number);
            self.watch(&lease, epoch, &run, None, &built)?;
        }

        Ok(RunLogTree::clone(&built.0))
    }

    pub(super) async fn tree_of(
        &self, lease: &ProviderLease, epoch: CacheEpoch, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<Built> {
        let request = hash_request(
            lease.id,
            lease.generation,
            &format!(
                "run-log-tree:{}:{}:{pipeline_id}:{run_number}",
                epoch.global, epoch.provider
            ),
        );
        let built = self
            .trees
            .deduplicate(request, || {
                self.build_tree(lease, epoch, pipeline_id, run_number)
            })
            .await?;
        if self.store.epoch(lease.id) == epoch {
            self.memoize(
                (
                    lease.id,
                    lease.generation,
                    epoch,
                    pipeline_id.to_string(),
                    run_number,
                ),
                built.clone(),
            );
        }

        Ok(built)
    }

    pub(super) async fn build_tree(
        &self, lease: &ProviderLease, epoch: CacheEpoch, pipeline_id: &str, run_number: i64,
    ) -> DomainResult<Built> {
        let run = self
            .provider_call(
                lease,
                lease.provider.fetch_run_details(pipeline_id, run_number),
            )
            .await?;
        let mut previous_tree = None;
        if run.status.is_terminal() {
            if let Some(stored) = self
                .store
                .read_tree(lease.id, pipeline_id, run_number)
                .await
            {
                if stored.run_status == run.status && stored.run_concluded_at == run.concluded_at {
                    if stored.log_tree_version == lease.provider.log_tree_version() {
                        return Ok((Arc::new(with_layout(stored.tree)), Arc::new(run)));
                    }
                    previous_tree = Some(stored.tree);
                }
            }
        }

        let mut nodes = match self
            .provider_call(lease, lease.provider.fetch_log_nodes(&run))
            .await
        {
            Ok(nodes) => nodes,
            Err(error) => {
                return previous_tree
                    .map(|tree| (Arc::new(with_layout(tree)), Arc::new(run)))
                    .ok_or(error);
            }
        };
        if let Some(mut previous) = previous_tree.filter(|tree| !tree.nodes.is_empty()) {
            let current_ids: HashSet<_> = nodes.iter().map(|node| node.id.as_str()).collect();
            previous
                .nodes
                .retain(|node| !current_ids.contains(node.id.as_str()));
            nodes.extend(previous.nodes.into_iter().map(|node| LogNode {
                id: node.id,
                parent_id: node.parent_id,
                kind: node.kind,
                name: node.name,
                status: node.status,
                started_at: node.started_at,
                finished_at: node.finished_at,
                log: node.log,
                web_url: node.web_url,
            }));
        }
        let tree = tree_from_nodes(&run, nodes);
        if tree.complete {
            let stored = StoredTree {
                log_tree_version: lease.provider.log_tree_version(),
                run_status: run.status.clone(),
                run_concluded_at: run.concluded_at,
                tree: tree.clone(),
            };
            let saved = self
                .provider_service
                .commit(lease, || {
                    self.store
                        .write_tree(lease.id, pipeline_id, run_number, &stored, epoch)
                })
                .await;
            if let Some(Err(e)) = saved {
                tracing::warn!(provider_id = lease.id, error = %e, "Failed to save the run log tree");
            }
        }

        Ok((Arc::new(tree), Arc::new(run)))
    }

    fn memoize(&self, key: TreeKey, built: Built) {
        let now = Instant::now();
        self.tree_memo
            .retain(|_, (at, _, _)| now.saturating_duration_since(*at) < COMPLETE_TREE_TTL);
        self.tree_memo.insert(key, (now, built.0, built.1));
    }

    pub(super) fn memoized(
        &self, lease: &ProviderLease, epoch: CacheEpoch, pipeline_id: &str, run_number: i64,
    ) -> Option<Built> {
        let entry = self.tree_memo.get(&(
            lease.id,
            lease.generation,
            epoch,
            pipeline_id.to_string(),
            run_number,
        ))?;
        let (at, tree, run) = entry.value();
        let ttl = if tree.complete {
            COMPLETE_TREE_TTL
        } else {
            OPEN_TREE_TTL
        };

        (Instant::now().saturating_duration_since(*at) < ttl)
            .then(|| (Arc::clone(tree), Arc::clone(run)))
    }
}

pub(super) fn tree_from_nodes(run: &PipelineRun, nodes: Vec<LogNode>) -> RunLogTree {
    let mut tree: Vec<RunLogNode> = Vec::with_capacity(nodes.len());
    let mut index = HashMap::with_capacity(nodes.len());
    for node in nodes {
        if index.contains_key(&node.id) {
            tracing::warn!(
                run_number = run.run_number,
                "Duplicate run log node ID ignored"
            );
            continue;
        }
        let parent = node
            .parent_id
            .as_ref()
            .and_then(|parent| index.get(parent).copied());
        let depth = parent.map_or(0, |parent: usize| tree[parent].depth.saturating_add(1));
        let duration_seconds = match (node.started_at, node.finished_at) {
            (Some(started), Some(finished)) => Some((finished - started).num_seconds()),
            _ => None,
        };
        index.insert(node.id.clone(), tree.len());
        tree.push(RunLogNode {
            navigable: node.log.is_some(),
            parent_id: parent.map(|_| node.parent_id.clone()).unwrap_or_default(),
            id: node.id,
            kind: node.kind,
            name: node.name,
            status: node.status,
            started_at: node.started_at,
            finished_at: node.finished_at,
            duration_seconds,
            log: node.log,
            web_url: node.web_url,
            depth,
        });
    }

    for position in (0..tree.len()).rev() {
        if !tree[position].navigable {
            continue;
        }
        let parent = tree[position]
            .parent_id
            .as_ref()
            .and_then(|parent| index.get(parent).copied());
        if let Some(parent) = parent {
            tree[parent].navigable = true;
        }
    }

    let complete = run.status.is_terminal()
        && tree.iter().all(|node| {
            node.status.is_terminal()
                && !matches!(
                    node.log,
                    Some(LogAvailability::Partial | LogAvailability::Pending)
                )
        });
    let with_log = || tree.iter().filter(|node| node.log.is_some());
    let default_node_id = with_log()
        .find(|node| node.status == PipelineStatus::Failed)
        .or_else(|| {
            with_log().find(|node| {
                matches!(
                    node.log,
                    Some(LogAvailability::Ready | LogAvailability::Partial)
                )
            })
        })
        .or_else(|| with_log().next())
        .map(|node| node.id.clone());

    let (groups, step_ids_by_node) = layout(&tree);
    RunLogTree {
        nodes: tree,
        default_node_id,
        complete,
        groups,
        step_ids_by_node,
    }
}

pub(super) fn with_layout(mut tree: RunLogTree) -> RunLogTree {
    (tree.groups, tree.step_ids_by_node) = layout(&tree.nodes);
    tree
}

fn layout(nodes: &[RunLogNode]) -> (Vec<RunLogGroup>, BTreeMap<String, Vec<String>>) {
    let index: HashMap<&str, &RunLogNode> =
        nodes.iter().map(|node| (node.id.as_str(), node)).collect();
    let mut members: Vec<(Option<&str>, Vec<&RunLogNode>)> = Vec::new();
    let mut step_ids_by_node: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for node in nodes {
        if let (LogNodeKind::Step, Some(parent)) = (node.kind, &node.parent_id) {
            step_ids_by_node
                .entry(parent.clone())
                .or_default()
                .push(node.id.clone());
        }
        if node.log.is_none() {
            continue;
        }
        let stage = stage_of(node, &index);
        match members.iter_mut().find(|(known, _)| *known == stage) {
            Some((_, group)) => group.push(node),
            None => members.push((stage, vec![node])),
        }
    }
    let groups = members
        .into_iter()
        .map(|(stage, group)| {
            let (failed, others): (Vec<&RunLogNode>, Vec<&RunLogNode>) = group
                .iter()
                .copied()
                .partition(|node| node.status == PipelineStatus::Failed);
            RunLogGroup {
                stage: stage.map(str::to_string),
                node_ids: group.iter().map(|node| node.id.clone()).collect(),
                failed_first_node_ids: failed
                    .into_iter()
                    .chain(others)
                    .map(|node| node.id.clone())
                    .collect(),
            }
        })
        .collect();
    (groups, step_ids_by_node)
}

fn stage_of<'a>(node: &RunLogNode, index: &HashMap<&str, &'a RunLogNode>) -> Option<&'a str> {
    let mut parent = node
        .parent_id
        .as_deref()
        .and_then(|id| index.get(id).copied());
    for _ in 0..index.len() {
        let current = parent?;
        if current.kind == LogNodeKind::Stage {
            return Some(&current.name);
        }
        parent = current
            .parent_id
            .as_deref()
            .and_then(|id| index.get(id).copied());
    }
    None
}

#[cfg(test)]
mod tests;
