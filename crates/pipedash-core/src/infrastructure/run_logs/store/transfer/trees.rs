use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use serde::Deserialize;

use super::super::registry::lock;
use super::super::tree::encode_tree;
use super::super::{
    DocKey,
    LogStore,
    RunId,
    StoredTree,
    TREE_FILE,
};
use super::transfer_error;
use crate::domain::{
    DomainError,
    DomainResult,
};

#[derive(Deserialize)]
struct TreeSnapshot {
    #[serde(default)]
    pipeline_id: Option<String>,
    #[serde(default)]
    run_number: Option<i64>,
    #[serde(flatten)]
    tree: StoredTree,
}

pub(super) struct TreeCopy {
    pub(super) run_id: RunId,
    pub(super) json: Vec<u8>,
}

pub(super) async fn snapshots(
    source: &Arc<LogStore>, provider_ids: &HashMap<i64, i64>, identities: &HashMap<RunId, DocKey>,
    remap: &impl Fn(&DocKey) -> Option<DocKey>, same_root: bool,
) -> DomainResult<Vec<TreeCopy>> {
    let runs: Vec<_> = {
        let registry = lock(&source.registry);
        registry
            .runs
            .iter()
            .filter(|(id, _)| provider_ids.contains_key(&id.0))
            .map(|(id, run)| (id.clone(), run.dir.join(TREE_FILE)))
            .collect()
    };
    let mut trees = Vec::new();
    for (run_id, path) in runs {
        let bytes = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(error) => {
                if error.kind() != io::ErrorKind::NotFound {
                    tracing::warn!(path = %path.display(), %error, "Skipping an unreadable cached log tree");
                }
                continue;
            }
        };
        let snapshot = match serde_json::from_slice::<TreeSnapshot>(&bytes) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "Skipping an unreadable cached log tree");
                continue;
            }
        };
        let identity = snapshot
            .pipeline_id
            .zip(snapshot.run_number)
            .map(|(pipeline_id, run_number)| DocKey {
                provider_id: run_id.0,
                pipeline_id,
                run_number,
                node_id: String::new(),
            })
            .or_else(|| identities.get(&run_id).cloned());
        let (mapped_run, json) = if let Some(identity) = identity {
            if identity.run_id() != run_id {
                tracing::warn!(path = %path.display(), "Skipping a cached log tree with a mismatched run identity");
                continue;
            }
            let Some(mapped) = remap(&identity) else {
                continue;
            };
            let json = encode_tree(&mapped.pipeline_id, mapped.run_number, &snapshot.tree)?;
            (mapped.run_id(), json)
        } else {
            let Some(provider_id) = provider_ids.get(&run_id.0).copied() else {
                continue;
            };
            if provider_id != run_id.0 {
                return Err(DomainError::InvalidConfig(format!(
                    "Cannot remap cached tree {}: its legacy entry has no run identity",
                    path.display()
                )));
            }
            let json =
                serde_json::to_vec(&snapshot.tree).map_err(|error| transfer_error(error.into()))?;
            ((provider_id, run_id.1.clone()), json)
        };
        if same_root {
            if mapped_run == run_id {
                continue;
            }
            if lock(&source.registry).runs.contains_key(&mapped_run) {
                return Err(transfer_error(io::Error::from(
                    io::ErrorKind::AlreadyExists,
                )));
            }
        }
        trees.push(TreeCopy {
            run_id: mapped_run,
            json,
        });
    }
    Ok(trees)
}
