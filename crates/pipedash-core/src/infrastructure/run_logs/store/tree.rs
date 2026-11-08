use std::collections::HashMap;
use std::fs::{
    self,
    OpenOptions,
};
use std::io;
use std::sync::Arc;
use std::time::SystemTime;

use super::registry::{
    lock,
    RunEntry,
};
use super::{
    io_error,
    join_error,
    run_key_of,
    CacheEpoch,
    LogStore,
    RunId,
    StoredTree,
    TOUCH_FILE,
    TREE_FILE,
    TREE_TMP_FILE,
};
use crate::domain::{
    DomainError,
    DomainResult,
};

impl LogStore {
    pub async fn read_tree(
        self: &Arc<Self>, provider_id: i64, pipeline_id: &str, run_number: i64,
    ) -> Option<StoredTree> {
        let path = self
            .run_dir(provider_id, &run_key_of(pipeline_id, run_number))
            .join(TREE_FILE);
        tokio::task::spawn_blocking(move || serde_json::from_slice(&fs::read(path).ok()?).ok())
            .await
            .ok()
            .flatten()
    }

    pub async fn write_tree(
        self: &Arc<Self>, provider_id: i64, pipeline_id: &str, run_number: i64, tree: &StoredTree,
        epoch: CacheEpoch,
    ) -> DomainResult<()> {
        let json = serde_json::to_vec(tree)
            .map_err(|e| DomainError::InternalError(format!("Couldn't save the log tree: {e}")))?;
        let store = self.clone();
        let run_id = (provider_id, run_key_of(pipeline_id, run_number));
        tokio::task::spawn_blocking(move || store.write_tree_blocking(run_id, &json, epoch))
            .await
            .map_err(join_error)?
    }

    fn write_tree_blocking(
        &self, run_id: RunId, json: &[u8], epoch: CacheEpoch,
    ) -> DomainResult<()> {
        let save_error = |e: io::Error| io_error("Couldn't save the log tree", &e);
        let mut reg = lock(&self.registry);
        if reg.epoch(run_id.0) != epoch {
            return Ok(());
        }
        let run_dir = self.run_dir(run_id.0, &run_id.1);
        fs::create_dir_all(&run_dir).map_err(save_error)?;
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(run_dir.join(TOUCH_FILE))
            .map_err(save_error)?;
        let tmp = run_dir.join(TREE_TMP_FILE);
        fs::write(&tmp, json).map_err(save_error)?;
        fs::rename(&tmp, run_dir.join(TREE_FILE)).map_err(save_error)?;
        let now = SystemTime::now();
        let run = reg.runs.entry(run_id).or_insert_with(|| RunEntry {
            dir: run_dir,
            tree_bytes: 0,
            last_access: now,
            last_touch: now,
            docs: HashMap::new(),
        });
        run.tree_bytes = json.len() as u64;
        Ok(())
    }
}
