use std::path::Path;
use std::sync::atomic::{
    AtomicBool,
    AtomicU64,
};
use std::sync::Arc;

use bytes::Bytes;
use pipedash_plugin_actions::ActionsLogDialect;
use tokio::sync::mpsc;

use super::{
    BlockCache,
    BlockLimits,
    DocKey,
    IngestOutcome,
    Ingester,
    StepBound,
    BLOCK_CACHE_BUDGET_BYTES,
};
use crate::domain::LogCursor;

static DIALECT: ActionsLogDialect = ActionsLogDialect;

pub(crate) fn doc_key() -> DocKey {
    DocKey {
        provider_id: 1,
        pipeline_id: "github__1__owner/repo/ci.yml".into(),
        run_number: 7,
        node_id: "100".into(),
    }
}

pub(crate) fn small_limits() -> BlockLimits {
    BlockLimits {
        lines: 4,
        target_bytes: 64,
        max_doc_text_bytes: u64::MAX,
    }
}

pub(crate) fn cache() -> BlockCache {
    BlockCache::new(BLOCK_CACHE_BUDGET_BYTES)
}

pub(crate) fn ingester(
    dir: &Path, key: DocKey, revision: i64, steps: Vec<StepBound>, limits: BlockLimits,
) -> Ingester {
    Ingester {
        key,
        revision,
        complete: true,
        dir: dir.to_path_buf(),
        dialect: &DIALECT,
        steps,
        received: Arc::new(AtomicU64::new(0)),
        limits,
        base: None,
        restart: Arc::new(AtomicBool::new(false)),
    }
}

pub(crate) fn feed(ingester: Ingester, data: &[u8], chunk: usize) -> super::Ingested {
    let chunks: Vec<Bytes> = data
        .chunks(chunk.max(1))
        .map(Bytes::copy_from_slice)
        .collect();
    let (tx, mut rx) = mpsc::channel(chunks.len() + 1);
    for chunk in chunks {
        tx.try_send(chunk).expect("channel has room");
    }
    drop(tx);
    ingester.run(&mut rx).expect("ingest")
}

pub(crate) fn finish(ingested: super::Ingested, cursor: Option<LogCursor>) -> IngestOutcome {
    ingested.finish(cursor).expect("finish")
}

pub(crate) fn ingest_for(
    dir: &Path, key: DocKey, revision: i64, data: &[u8], chunk: usize, steps: Vec<StepBound>,
    limits: BlockLimits,
) -> IngestOutcome {
    finish(
        feed(ingester(dir, key, revision, steps, limits), data, chunk),
        None,
    )
}

pub(crate) fn ingest(
    dir: &Path, data: &[u8], chunk: usize, steps: Vec<StepBound>, limits: BlockLimits,
) -> IngestOutcome {
    ingest_for(dir, doc_key(), 1, data, chunk, steps, limits)
}
