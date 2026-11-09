use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{
    Path,
    PathBuf,
};

use super::{
    io_error,
    join_error,
    DocKey,
    LogStore,
    PublishOutcome,
    StoredDocMeta,
    StoredTree,
    DOCS_DIR,
    LAYOUT_DIR,
    STORED_FORMAT,
    TREE_FILE,
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

impl LogStore {
    pub(crate) async fn transfer_completed(
        source_root: PathBuf, target_root: PathBuf, remap: impl Fn(&DocKey) -> Option<DocKey>,
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
        let documents = tokio::task::spawn_blocking(move || completed_documents(&source_root))
            .await
            .map_err(join_error)??;
        let mut documents = documents
            .into_iter()
            .filter_map(|(dir, meta)| {
                let key = remap(&meta.key)?;
                if same_root && key == meta.key {
                    return None;
                }
                Some((dir, meta, key))
            })
            .peekable();
        if documents.peek().is_none() {
            return Ok(0);
        }
        let target = LogStore::open(target_root, u64::MAX).await?;
        let mut trees = HashSet::new();
        let mut count = 0;
        for (dir, mut meta, key) in documents {
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
            let copy_tree = trees.insert(key.run_id());
            let copy_tmp = tmp.clone();
            let copied = tokio::task::spawn_blocking(move || {
                copy_document(&dir, &copy_tmp, &meta, copy_tree)
            })
            .await
            .map_err(join_error)
            .and_then(|result| result);
            let tree = match copied {
                Ok(tree) => tree,
                Err(error) => {
                    target.discard(tmp).await;
                    return Err(error);
                }
            };
            let published = target
                .publish(&key, &tmp, revision, epoch, current, HashSet::new())
                .await;
            target.discard(tmp).await;
            if published? != PublishOutcome::Published {
                return Err(DomainError::InternalError(
                    "The log cache changed during migration".into(),
                ));
            }
            if let Some(tree) = tree {
                target
                    .write_tree(
                        key.provider_id,
                        &key.pipeline_id,
                        key.run_number,
                        &tree,
                        epoch,
                    )
                    .await?;
            }
            count += 1;
        }
        Ok(count)
    }
}

fn transfer_error(error: io::Error) -> DomainError {
    io_error("Couldn't transfer cached logs", &error)
}

fn directories(path: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut directories = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            directories.push(entry.path());
        }
    }
    Ok(directories)
}

fn completed_documents(root: &Path) -> DomainResult<Vec<(PathBuf, StoredDocMeta)>> {
    let mut documents = Vec::new();
    for provider in directories(&root.join(LAYOUT_DIR)).map_err(transfer_error)? {
        if provider
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.parse::<i64>().ok())
            .is_none()
        {
            continue;
        }
        for run in directories(&provider).map_err(transfer_error)? {
            for doc in directories(&run.join(DOCS_DIR)).map_err(transfer_error)? {
                if let Some(document) = completed_document(&doc).map_err(transfer_error)? {
                    documents.push(document);
                }
            }
        }
    }
    Ok(documents)
}

fn completed_document(dir: &Path) -> io::Result<Option<(PathBuf, StoredDocMeta)>> {
    let latest = directories(dir)?
        .into_iter()
        .filter_map(|path| {
            let revision = path.file_name()?.to_str()?.parse::<i64>().ok()?;
            Some((revision, path))
        })
        .max_by_key(|(revision, _)| *revision);
    let Some((revision, dir)) = latest else {
        return Ok(None);
    };
    let bytes = match fs::read(dir.join(META_FILE)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let Ok(meta) = serde_json::from_slice::<StoredDocMeta>(&bytes) else {
        return Ok(None);
    };
    Ok(
        (meta.format == STORED_FORMAT && meta.complete && meta.revision == revision)
            .then_some((dir, meta)),
    )
}

fn copy_document(
    dir: &Path, tmp: &Path, meta: &StoredDocMeta, copy_tree: bool,
) -> DomainResult<Option<StoredTree>> {
    for file in [BLOCKS_FILE, INDEX_FILE] {
        fs::copy(dir.join(file), tmp.join(file)).map_err(transfer_error)?;
    }
    let json = serde_json::to_vec(meta).map_err(|error| transfer_error(error.into()))?;
    fs::write(tmp.join(META_FILE), json).map_err(transfer_error)?;
    if !copy_tree {
        return Ok(None);
    }
    let Some(run) = dir.ancestors().nth(3) else {
        return Ok(None);
    };
    match fs::read(run.join(TREE_FILE)) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|error| transfer_error(error.into())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(transfer_error(error)),
    }
}
