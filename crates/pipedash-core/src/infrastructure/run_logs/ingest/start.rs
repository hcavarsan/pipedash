use std::fs::{
    self,
    File,
};
use std::io;

use super::super::ansi::StyleTable;
use super::super::checkpoint::{
    copy_prefix,
    Base,
    TAIL_FILE,
};
use super::super::outline::Outline;
use super::{
    Ingester,
    BLOCKS_FILE,
    INDEX_FILE,
};

pub(super) struct Start {
    pub(super) outline: Outline,
    pub(super) styles: StyleTable,
    pub(super) blocks: File,
    pub(super) index: File,
    pub(super) blocks_len: u64,
    pub(super) index_len: u64,
    pub(super) line_count: u32,
    pub(super) text_bytes: u64,
    pub(super) first_ts: Option<i64>,
    pub(super) pending: Vec<u8>,
    pub(super) previous: Option<(i64, u32)>,
}

impl Start {
    pub(super) fn fresh(cfg: &mut Ingester) -> io::Result<Self> {
        Ok(Self {
            outline: Outline::new(std::mem::take(&mut cfg.steps), &cfg.dir),
            styles: StyleTable::default(),
            blocks: File::create(cfg.dir.join(BLOCKS_FILE))?,
            index: File::create(cfg.dir.join(INDEX_FILE))?,
            blocks_len: 0,
            index_len: 0,
            line_count: 0,
            text_bytes: 0,
            first_ts: None,
            pending: Vec::new(),
            previous: None,
        })
    }

    pub(super) fn resume(cfg: &mut Ingester) -> io::Result<Self> {
        let missing = || {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "There is no log checkpoint to resume from",
            )
        };
        let Base {
            revision,
            document,
            checkpoint,
            ..
        } = cfg.base.as_mut().ok_or_else(missing)?;
        let resume = checkpoint
            .as_mut()
            .and_then(|checkpoint| checkpoint.resume.as_mut())
            .ok_or_else(missing)?;
        let from = document.dir();
        let pending = match fs::read(from.join(TAIL_FILE)) {
            Ok(tail) => tail,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };

        Ok(Self {
            blocks: copy_prefix(
                &from.join(BLOCKS_FILE),
                &cfg.dir.join(BLOCKS_FILE),
                resume.blocks_len,
            )?,
            index: copy_prefix(
                &from.join(INDEX_FILE),
                &cfg.dir.join(INDEX_FILE),
                resume.index_len,
            )?,
            outline: std::mem::take(&mut resume.outline).resume(
                std::mem::take(&mut cfg.steps),
                from,
                &cfg.dir,
            )?,
            styles: StyleTable::from_list(document.meta().styles[..resume.styles].to_vec()),
            blocks_len: resume.blocks_len,
            index_len: resume.index_len,
            line_count: resume.line_count,
            text_bytes: resume.text_bytes,
            first_ts: resume.first_ts,
            pending,
            previous: Some((*revision, resume.line_count)),
        })
    }
}
