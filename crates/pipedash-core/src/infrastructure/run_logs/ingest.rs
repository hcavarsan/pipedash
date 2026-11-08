use std::io;
use std::path::PathBuf;
use std::sync::atomic::{
    AtomicBool,
    AtomicU64,
    Ordering,
};
use std::sync::Arc;

use bytes::Bytes;
use tokio::sync::mpsc;

use self::start::Start;
use self::state::State;
use super::checkpoint::{
    self,
    steps_hash,
    Base,
};
use super::outline::StepBound;
use super::store::{
    DocKey,
    StoredDocMeta,
};
use super::{
    BLOCK_MAX_LINES,
    BLOCK_TARGET_BYTES,
    MAX_DOC_TEXT_BYTES,
};
use crate::domain::{
    LogCursor,
    LogDialect,
    LogRead,
    LogReadMode,
};

mod start;
mod state;
#[cfg(test)]
mod tests;

pub(super) const BLOCKS_FILE: &str = "blocks.bin";
pub(super) const INDEX_FILE: &str = "index.bin";
pub(super) const META_FILE: &str = "meta.json";
const BLOCKS_BUFFER_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct BlockLimits {
    pub lines: usize,
    pub target_bytes: usize,
    pub max_doc_text_bytes: u64,
}

impl Default for BlockLimits {
    fn default() -> Self {
        Self {
            lines: BLOCK_MAX_LINES,
            target_bytes: BLOCK_TARGET_BYTES,
            max_doc_text_bytes: MAX_DOC_TEXT_BYTES,
        }
    }
}

pub struct Ingester {
    pub key: DocKey,
    pub revision: i64,
    pub complete: bool,
    pub dir: PathBuf,
    pub dialect: &'static dyn LogDialect,
    pub steps: Vec<StepBound>,
    pub received: Arc<AtomicU64>,
    pub limits: BlockLimits,
    pub base: Option<Base>,
    pub restart: Arc<AtomicBool>,
}

#[derive(Debug)]
pub struct IngestOutcome {
    pub meta: StoredDocMeta,
    pub truncated: bool,
    pub resumable: bool,
}

#[derive(PartialEq, Eq)]
enum Flow {
    Continue,
    Stop,
}

pub struct Ingested {
    cfg: Ingester,
    resumed: Option<bool>,
    state: Option<State>,
    steps_hash: String,
}

impl Ingester {
    pub fn run(self, rx: &mut mpsc::Receiver<Bytes>) -> io::Result<Ingested> {
        let received = Arc::clone(&self.received);
        let mut ingested = Ingested {
            steps_hash: steps_hash(&self.steps),
            cfg: self,
            resumed: None,
            state: None,
        };
        while let Some(chunk) = rx.blocking_recv() {
            received.fetch_add(chunk.len() as u64, Ordering::Relaxed);
            if ingested.start()?.push(&chunk)? == Flow::Stop {
                break;
            }
        }
        if !ingested.decide() {
            ingested.start()?;
        }
        if let Some(state) = ingested.state.as_mut() {
            state.seal();
            if state.truncated {
                rx.close();
            }
        }

        Ok(ingested)
    }
}

impl Ingested {
    pub fn resumed(&self) -> bool {
        self.resumed == Some(true)
    }

    pub fn truncated(&self) -> bool {
        self.state.as_ref().is_some_and(|state| state.truncated)
    }

    pub fn unchanged(&self, read: &LogRead) -> bool {
        let Some(base) = &self.cfg.base else {
            return false;
        };
        let Some(prior) = &base.checkpoint else {
            return false;
        };
        if base.document.meta().complete != self.cfg.complete || prior.steps_hash != self.steps_hash
        {
            return false;
        }
        let state = self.state.as_ref();
        match read.mode {
            LogReadMode::Append => {
                state.is_none_or(|state| state.new_bytes == 0)
                    && read.cursor.as_ref().is_some_and(|cursor| {
                        prior
                            .resume
                            .as_ref()
                            .is_some_and(|resume| &resume.cursor == cursor)
                    })
            }
            LogReadMode::Replace => {
                let hash = state.and_then(|state| state.raw_hash.as_deref());
                hash.is_some() && hash == prior.raw_hash.as_deref()
            }
        }
    }

    pub fn finish(mut self, cursor: Option<LogCursor>) -> io::Result<IngestOutcome> {
        self.start()?;
        let Self {
            cfg,
            state,
            steps_hash,
            ..
        } = self;
        let mut state = state.ok_or_else(not_started)?;
        let had_cursor = cursor.is_some();
        let mut keep =
            !cfg.complete && !state.truncated && (had_cursor || state.raw_hash.is_some());
        if keep {
            state.write_checkpoint(&cfg.dir, cursor, &steps_hash)?;
        }
        state.render_tail()?;
        if keep && state.truncated {
            checkpoint::remove(&cfg.dir);
            keep = false;
        }

        state.finish(cfg, keep && had_cursor)
    }

    fn decide(&mut self) -> bool {
        let Self { cfg, resumed, .. } = self;
        *resumed.get_or_insert_with(|| {
            cfg.base
                .as_ref()
                .is_some_and(|base| base.cursor(&cfg.steps).is_some())
                && !cfg.restart.load(Ordering::SeqCst)
        })
    }

    fn start(&mut self) -> io::Result<&mut State> {
        if self.state.is_none() {
            let start = if self.decide() {
                Start::resume(&mut self.cfg)?
            } else {
                Start::fresh(&mut self.cfg)?
            };
            self.state = Some(State::new(&self.cfg, start));
        }

        self.state.as_mut().ok_or_else(not_started)
    }
}

fn not_started() -> io::Error {
    io::Error::other("The log ingest has not started")
}
