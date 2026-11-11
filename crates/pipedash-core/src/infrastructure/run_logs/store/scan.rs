use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{
    Path,
    PathBuf,
};
use std::sync::atomic::{
    AtomicBool,
    AtomicU32,
};
use std::sync::Arc;
use std::time::SystemTime;

use super::registry::{
    DocEntry,
    Registry,
    RevisionState,
    RunEntry,
};
use super::{
    dir_size,
    remove_file_quiet,
    StoredDocMeta,
    DOCS_DIR,
    LAYOUT_DIR,
    STORED_FORMAT,
    TMP_DIR,
    TOUCH_FILE,
    TREE_FILE,
    TREE_TMP_FILE,
};
use crate::infrastructure::run_logs::ingest::META_FILE;
use crate::infrastructure::run_logs::CACHE_MAX_IDLE;

fn valid_meta(dir: &Path) -> bool {
    fs::read(dir.join(META_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<StoredDocMeta>(&bytes).ok())
        .is_some_and(|meta| meta.format == STORED_FORMAT)
}

fn remove_path(path: &Path, cleanup: bool) {
    if !cleanup {
        return;
    }
    let result = if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    if let Err(e) = result {
        if e.kind() != io::ErrorKind::NotFound {
            tracing::warn!("Couldn't remove a stale log cache entry: {e}");
        }
    }
}

pub(super) fn scan_preserving(root: &Path, now: SystemTime) -> io::Result<Registry> {
    scan_mode(root, now, false)
}

pub(super) fn scan(root: &Path, now: SystemTime) -> io::Result<Registry> {
    scan_mode(root, now, true)
}

fn scan_mode(root: &Path, now: SystemTime, cleanup: bool) -> io::Result<Registry> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_name() != LAYOUT_DIR {
            remove_path(&entry.path(), cleanup);
        }
    }
    let base = root.join(LAYOUT_DIR);
    if cleanup {
        fs::create_dir_all(&base)?;
    } else if !base.is_dir() {
        return Ok(Registry::default());
    }
    remove_path(&base.join(TMP_DIR), cleanup);

    let mut reg = Registry::default();
    for provider in fs::read_dir(&base)? {
        let provider = provider?;
        let provider_dir = provider.path();
        let Some(provider_id) = provider
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<i64>().ok())
        else {
            remove_path(&provider_dir, cleanup);
            continue;
        };
        if !provider_dir.is_dir() {
            remove_path(&provider_dir, cleanup);
            continue;
        }
        for run in fs::read_dir(&provider_dir)? {
            let run = run?;
            let run_dir = run.path();
            let Some(run_key) = run.file_name().to_str().map(str::to_owned) else {
                remove_path(&run_dir, cleanup);
                continue;
            };
            match scan_run(&run_dir, now, cleanup) {
                Some((entry, newest)) => {
                    reg.last_revision = reg.last_revision.max(newest);
                    reg.runs.insert((provider_id, run_key), entry);
                }
                None => remove_path(&run_dir, cleanup),
            }
        }
        if cleanup {
            let _ = fs::remove_dir(&provider_dir);
        }
    }
    Ok(reg)
}

fn scan_run(run_dir: &Path, now: SystemTime, cleanup: bool) -> Option<(RunEntry, i64)> {
    if !run_dir.is_dir() {
        return None;
    }
    let touch = fs::metadata(run_dir.join(TOUCH_FILE))
        .and_then(|m| m.modified())
        .or_else(|_| fs::metadata(run_dir).and_then(|m| m.modified()))
        .unwrap_or(now);
    if cleanup && now.duration_since(touch).unwrap_or_default() > CACHE_MAX_IDLE {
        return None;
    }
    if cleanup {
        remove_file_quiet(&run_dir.join(TREE_TMP_FILE));
    }
    let tree_bytes = fs::metadata(run_dir.join(TREE_FILE)).map_or(0, |m| m.len());

    let mut docs = HashMap::new();
    let mut newest = 0;
    let docs_dir = run_dir.join(DOCS_DIR);
    for doc in fs::read_dir(&docs_dir).into_iter().flatten().flatten() {
        let doc_dir = doc.path();
        let Some(doc_key) = doc.file_name().to_str().map(str::to_owned) else {
            remove_path(&doc_dir, cleanup);
            continue;
        };
        let mut revisions: Vec<(i64, PathBuf)> = Vec::new();
        for revision in fs::read_dir(&doc_dir).into_iter().flatten().flatten() {
            match revision
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<i64>().ok())
            {
                Some(number) if revision.path().is_dir() => {
                    revisions.push((number, revision.path()))
                }
                _ => remove_path(&revision.path(), cleanup),
            }
        }
        revisions.sort_by_key(|(number, _)| std::cmp::Reverse(*number));
        let keep = revisions.iter().position(|(_, dir)| valid_meta(dir));
        let mut entry = DocEntry::new(touch);
        for (position, (number, dir)) in revisions.into_iter().enumerate() {
            if Some(position) == keep {
                let state = RevisionState {
                    bytes: dir_size(&dir),
                    dir,
                    readers: AtomicU32::new(0),
                    doomed: AtomicBool::new(false),
                };
                entry.current = Some(number);
                entry.revisions.insert(number, Arc::new(state));
                newest = newest.max(number);
            } else {
                remove_path(&dir, cleanup);
            }
        }
        if entry.current.is_some() {
            docs.insert(doc_key, entry);
        } else {
            remove_path(&doc_dir, cleanup);
        }
    }
    if docs.is_empty() && tree_bytes == 0 {
        return None;
    }
    let run = RunEntry {
        dir: run_dir.to_path_buf(),
        tree_bytes,
        last_access: touch,
        last_touch: touch,
        docs,
    };
    Some((run, newest))
}
