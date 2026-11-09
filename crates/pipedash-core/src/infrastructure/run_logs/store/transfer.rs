use std::collections::{
    HashMap,
    HashSet,
};
use std::fs;
use std::io;
use std::path::{
    Path,
    PathBuf,
};

use super::registry::lock;
use super::{
    io_error,
    join_error,
    DocKey,
    LogStore,
    PublishOutcome,
    ReadLease,
    RunId,
    StoredDocMeta,
    STORED_FORMAT,
};
use crate::domain::{
    DomainError,
    DomainResult,
};
use crate::infrastructure::run_logs::ingest::{
    BLOCKS_FILE,
    INDEX_FILE,
    META_FILE,
};

mod trees;

struct PinnedDocument {
    meta: StoredDocMeta,
    lease: ReadLease,
}

impl LogStore {
    pub(crate) async fn transfer_completed(
        source_root: PathBuf, target_root: PathBuf, provider_ids: &HashMap<i64, i64>,
        remap: impl Fn(&DocKey) -> Option<DocKey>,
    ) -> DomainResult<usize> {
        let source_root = match tokio::fs::canonicalize(source_root).await {
            Ok(root) => root,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(error) => return Err(transfer_error(error)),
        };
        let same_root = match tokio::fs::canonicalize(&target_root).await {
            Ok(root) => root == source_root,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => return Err(transfer_error(error)),
        };
        let source = LogStore::open_preserving(source_root).await?;
        let snapshot = source.clone();
        let documents = tokio::task::spawn_blocking(move || stored_documents(&snapshot))
            .await
            .map_err(join_error)??;
        let mut identities = HashMap::<RunId, DocKey>::new();
        let mut pinned = Vec::new();
        for meta in documents {
            if !provider_ids.contains_key(&meta.key.provider_id) {
                continue;
            }
            identities
                .entry(meta.key.run_id())
                .or_insert_with(|| meta.key.clone());
            if !meta.complete {
                continue;
            }
            let Some((document, lease)) = source.lease(&meta.key, meta.revision).await? else {
                continue;
            };
            if document.meta().complete {
                pinned.push(PinnedDocument { meta, lease });
            }
        }
        let trees = trees::snapshots(&source, provider_ids, &identities, &remap, same_root).await?;
        if pinned.is_empty() && trees.is_empty() {
            return Ok(0);
        }
        let target = LogStore::open(target_root, u64::MAX).await?;
        let mut count = 0;
        for PinnedDocument { mut meta, lease } in pinned {
            let Some(key) = remap(&meta.key) else {
                continue;
            };
            if same_root && key == meta.key {
                continue;
            }
            let current = target.current(&key);
            if same_root && current.is_some() {
                return Err(transfer_error(io::Error::from(
                    io::ErrorKind::AlreadyExists,
                )));
            }
            if current.is_some() {
                meta.revision = target.allocate_revision();
            }
            meta.key = key.clone();
            let revision = meta.revision;
            let epoch = target.epoch(key.provider_id);
            let tmp = target.tmp_dir().await?;
            let copy_tmp = tmp.clone();
            let dir = lease.state.dir.clone();
            let copied = tokio::task::spawn_blocking(move || copy_document(&dir, &copy_tmp, &meta))
                .await
                .map_err(join_error)
                .and_then(|result| result);
            if let Err(error) = copied {
                target.discard(tmp).await;
                return Err(error);
            }
            let published = target
                .publish(&key, &tmp, revision, epoch, current, HashSet::new())
                .await;
            target.discard(tmp).await;
            if published? != PublishOutcome::Published {
                return Err(DomainError::InternalError(
                    "The log cache changed during migration".into(),
                ));
            }
            drop(lease);
            count += 1;
        }
        for tree in trees {
            let store = target.clone();
            let epoch = target.epoch(tree.run_id.0);
            tokio::task::spawn_blocking(move || {
                store.write_tree_blocking(tree.run_id, &tree.json, epoch)
            })
            .await
            .map_err(join_error)??;
            count += 1;
        }
        Ok(count)
    }
}

fn transfer_error(error: io::Error) -> DomainError {
    io_error("Couldn't transfer cached logs", &error)
}

fn stored_documents(source: &LogStore) -> DomainResult<Vec<StoredDocMeta>> {
    let snapshots: Vec<_> = {
        let registry = lock(&source.registry);
        registry
            .runs
            .values()
            .flat_map(|run| run.docs.values())
            .filter_map(|doc| {
                let revision = doc.current?;
                Some((doc.revisions.get(&revision)?.dir.clone(), revision))
            })
            .collect()
    };
    let mut documents = Vec::new();
    for (dir, revision) in snapshots {
        if let Some(meta) = stored_document(&dir, revision).map_err(transfer_error)? {
            documents.push(meta);
        }
    }
    Ok(documents)
}

fn stored_document(dir: &Path, revision: i64) -> io::Result<Option<StoredDocMeta>> {
    let bytes = match fs::read(dir.join(META_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let Ok(meta) = serde_json::from_slice::<StoredDocMeta>(&bytes) else {
        return Ok(None);
    };
    Ok((meta.format == STORED_FORMAT && meta.revision == revision).then_some(meta))
}

fn copy_document(dir: &Path, tmp: &Path, meta: &StoredDocMeta) -> DomainResult<()> {
    for file in [BLOCKS_FILE, INDEX_FILE] {
        fs::copy(dir.join(file), tmp.join(file)).map_err(transfer_error)?;
    }
    let json = serde_json::to_vec(meta).map_err(|error| transfer_error(error.into()))?;
    fs::write(tmp.join(META_FILE), json).map_err(transfer_error)
}
