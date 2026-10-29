use std::fs::{
    self,
    File,
    OpenOptions,
};
use std::io::{
    self,
    BufWriter,
    Write,
};
use std::path::Path;
use std::sync::Arc;

use serde::{
    Deserialize,
    Serialize,
};
use sha2::{
    Digest,
    Sha256,
};

use super::block::INDEX_RECORD_BYTES;
use super::document::Document;
use super::ingest::{
    BLOCKS_FILE,
    INDEX_FILE,
};
use super::outline::{
    Outline,
    StepBound,
};
use super::store::{
    short_hex,
    ReadLease,
};
use super::MAX_LINE_BYTES;
use crate::domain::LogCursor;

pub(super) const CHECKPOINT_FILE: &str = "checkpoint.json";
pub(super) const TAIL_FILE: &str = "tail.bin";

#[derive(Serialize, Deserialize)]
pub(super) struct Resume<O = Outline> {
    pub cursor: LogCursor,
    pub line_count: u32,
    pub text_bytes: u64,
    pub first_ts: Option<i64>,
    pub blocks_len: u64,
    pub index_len: u64,
    pub styles: usize,
    pub outline: O,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Checkpoint<O = Outline> {
    pub steps_hash: String,
    pub raw_hash: Option<String>,
    pub resume: Option<Resume<O>>,
}

pub(super) fn steps_hash(steps: &[StepBound]) -> String {
    let json = serde_json::to_vec(steps).unwrap_or_default();
    short_hex(&Sha256::digest(json))
}

pub(super) fn copy_prefix(from: &Path, to: &Path, len: u64) -> io::Result<File> {
    fs::copy(from, to)?;
    let file = OpenOptions::new().append(true).open(to)?;
    file.set_len(len)?;
    Ok(file)
}

pub(super) fn remove(dir: &Path) {
    for name in [CHECKPOINT_FILE, TAIL_FILE] {
        if let Err(e) = fs::remove_file(dir.join(name)) {
            if e.kind() != io::ErrorKind::NotFound {
                tracing::warn!("Couldn't remove a stale log checkpoint: {e}");
            }
        }
    }
}

impl Checkpoint<&Outline> {
    pub(super) fn write(&self, dir: &Path, tail: &[u8]) -> io::Result<()> {
        let mut writer = BufWriter::new(File::create(dir.join(CHECKPOINT_FILE))?);
        serde_json::to_writer(&mut writer, self).map_err(io::Error::from)?;
        writer
            .into_inner()
            .map_err(|e| e.into_error())?
            .sync_all()?;
        if !tail.is_empty() {
            let mut file = File::create(dir.join(TAIL_FILE))?;
            file.write_all(tail)?;
            file.sync_all()?;
        }
        Ok(())
    }
}

impl Checkpoint {
    pub(super) fn load(document: &Document) -> Option<Self> {
        let dir = document.dir();
        let checkpoint: Self =
            serde_json::from_slice(&fs::read(dir.join(CHECKPOINT_FILE)).ok()?).ok()?;
        let meta = document.meta();
        let fits = checkpoint.resume.as_ref().is_none_or(|resume| {
            let len = |name: &str| fs::metadata(dir.join(name)).map(|m| m.len()).ok();
            resume.styles <= meta.styles.len()
                && resume.line_count <= meta.line_count
                && resume.index_len % INDEX_RECORD_BYTES as u64 == 0
                && len(BLOCKS_FILE) >= Some(resume.blocks_len)
                && len(INDEX_FILE) >= Some(resume.index_len)
                && resume.outline.spool_fits(dir)
                && len(TAIL_FILE).unwrap_or(0) <= MAX_LINE_BYTES as u64
        });
        fits.then_some(checkpoint)
    }
}

pub struct Base {
    pub revision: i64,
    pub(super) document: Arc<Document>,
    pub(super) _lease: ReadLease,
    pub(super) checkpoint: Option<Checkpoint>,
}

impl Base {
    pub fn cursor(&self, steps: &[StepBound]) -> Option<&LogCursor> {
        let resume = self.checkpoint.as_ref()?.resume.as_ref()?;
        resume.outline.accepts(steps).then_some(&resume.cursor)
    }
}
