use std::fs::{
    self,
    File,
};
use std::io::{
    self,
    BufReader,
    BufWriter,
    Read,
    Seek,
    SeekFrom,
    Write,
};
use std::path::{
    Path,
    PathBuf,
};

use serde::{
    Deserialize,
    Serialize,
};

use super::checkpoint::copy_prefix;
use super::{
    MARKER_LINES_CAP,
    MAX_SECTIONS,
};
use crate::domain::{
    LineKind,
    LogSection,
    PipelineStatus,
};

mod sections;

const ERRORS_FILE: &str = "errors.bin";
const ERRORS_BUFFER_BYTES: usize = 64 * 1024;
const MAX_TITLE_BYTES: usize = 512;
const MAX_STEP_MARKERS: usize = 100_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepBound {
    pub node_id: String,
    pub title: String,
    pub status: PipelineStatus,
    pub started_ms: Option<i64>,
    pub finished_ms: Option<i64>,
}

#[derive(Serialize, Deserialize)]
struct Group {
    header: u32,
    title: String,
    end: Option<u32>,
    first_ms: Option<i64>,
    last_ms: Option<i64>,
    open: bool,
}

pub(crate) struct OutlineOutput {
    pub sections: Vec<LogSection>,
    pub error_lines: Vec<u32>,
    pub warning_lines: Vec<u32>,
    pub error_total: u32,
    pub warning_total: u32,
    pub focus_line: Option<u32>,
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Outline {
    steps: Vec<StepBound>,
    ts_boundary: Vec<u32>,
    markers: Vec<(u32, i64)>,
    groups: Vec<Group>,
    open_groups: Vec<usize>,
    ignored_depth: u32,
    warned: bool,
    last_ts: Option<i64>,
    #[serde(skip)]
    errors_path: PathBuf,
    #[serde(skip)]
    errors: Option<BufWriter<File>>,
    error_total: u32,
    warning_total: u32,
    error_lines: Vec<u32>,
    warning_lines: Vec<u32>,
}

fn floor_sec(ms: i64) -> i64 {
    ms.div_euclid(1000)
}

fn truncate_title(text: &str) -> String {
    if text.len() <= MAX_TITLE_BYTES {
        return text.to_string();
    }
    let mut end = MAX_TITLE_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

fn normalized(mut steps: Vec<StepBound>) -> Vec<StepBound> {
    steps.sort_by_key(|s| (s.started_ms.is_none(), s.started_ms));
    steps.truncate(MAX_SECTIONS);
    steps
}

impl Outline {
    pub(crate) fn new(steps: Vec<StepBound>, dir: &Path) -> Self {
        Self {
            steps: normalized(steps),
            errors_path: dir.join(ERRORS_FILE),
            ..Self::default()
        }
    }

    pub(crate) fn accepts(&self, steps: &[StepBound]) -> bool {
        let steps = normalized(steps.to_vec());
        let placed = self.ts_boundary.len();
        steps.len() >= placed
            && self
                .steps
                .iter()
                .zip(&steps)
                .take(placed)
                .all(|(old, new)| old.node_id == new.node_id && old.started_ms == new.started_ms)
    }

    fn spool_bytes(&self) -> u64 {
        u64::from(self.error_total) * 4
    }

    pub(crate) fn spool_fits(&self, dir: &Path) -> bool {
        self.error_total == 0
            || fs::metadata(dir.join(ERRORS_FILE)).is_ok_and(|m| m.len() >= self.spool_bytes())
    }

    pub(crate) fn resume(
        mut self, steps: Vec<StepBound>, base: &Path, dir: &Path,
    ) -> io::Result<Self> {
        self.steps = normalized(steps);
        self.errors_path = dir.join(ERRORS_FILE);
        if self.error_total > 0 {
            let spool = copy_prefix(
                &base.join(ERRORS_FILE),
                &self.errors_path,
                self.spool_bytes(),
            )?;
            self.errors = Some(BufWriter::with_capacity(ERRORS_BUFFER_BYTES, spool));
        }
        Ok(self)
    }

    pub(crate) fn discard_spool(&mut self) -> io::Result<()> {
        self.errors = None;
        match fs::remove_file(&self.errors_path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    pub(crate) fn line(
        &mut self, index: u32, kind: LineKind, ts: Option<i64>, text: &str, step_start: bool,
    ) -> io::Result<()> {
        if ts.is_some() {
            self.last_ts = ts;
        }
        if let Some(effective) = self.last_ts {
            let sec = floor_sec(effective);
            self.advance(index, sec);
            if step_start && self.markers.len() < MAX_STEP_MARKERS {
                self.markers.push((index, sec));
            }
        }
        match kind {
            LineKind::Error => {
                if self.error_lines.len() < MARKER_LINES_CAP {
                    self.error_lines.push(index);
                }
                self.error_total = self.error_total.saturating_add(1);
                if self.errors.is_none() {
                    let file = File::create(&self.errors_path)?;
                    self.errors = Some(BufWriter::with_capacity(ERRORS_BUFFER_BYTES, file));
                }
                if let Some(writer) = self.errors.as_mut() {
                    writer.write_all(&index.to_le_bytes())?;
                }
            }
            LineKind::Warning => {
                if self.warning_lines.len() < MARKER_LINES_CAP {
                    self.warning_lines.push(index);
                }
                self.warning_total = self.warning_total.saturating_add(1);
            }
            LineKind::Group => self.group_start(index, text),
            _ => {}
        }
        Ok(())
    }

    fn advance(&mut self, index: u32, sec: i64) {
        while self.ts_boundary.len() < self.steps.len() {
            let Some(started) = self.steps[self.ts_boundary.len()].started_ms else {
                break;
            };
            if sec < floor_sec(started) {
                break;
            }
            self.ts_boundary.push(index);
        }
    }

    fn group_start(&mut self, index: u32, text: &str) {
        if self.steps.len() + self.groups.len() < MAX_SECTIONS {
            self.groups.push(Group {
                header: index,
                title: truncate_title(text),
                end: None,
                first_ms: self.last_ts,
                last_ms: None,
                open: false,
            });
            self.open_groups.push(self.groups.len() - 1);
        } else {
            if !self.warned {
                self.warned = true;
                tracing::warn!("Run log has more than {MAX_SECTIONS} sections; ignoring groups");
            }
            self.ignored_depth += 1;
        }
    }

    pub(crate) fn group_end(&mut self, line_count: u32, ts: Option<i64>) {
        if ts.is_some() {
            self.last_ts = ts;
        }
        if self.ignored_depth > 0 {
            self.ignored_depth -= 1;
            return;
        }
        if let Some(group) = self.open_groups.pop() {
            let group = &mut self.groups[group];
            group.end = Some(line_count);
            group.last_ms = self.last_ts;
        }
    }

    pub(crate) fn end_groups(&mut self, line_count: u32) {
        self.ignored_depth = 0;
        for group in self.open_groups.drain(..) {
            let group = &mut self.groups[group];
            group.end = Some(line_count);
            group.last_ms = self.last_ts;
        }
    }

    pub(crate) fn open_current_group(&mut self, title: &str) {
        let Some(&group) = self.open_groups.last() else {
            return;
        };
        let group = &mut self.groups[group];
        group.open = true;
        if !title.trim().is_empty() {
            group.title = truncate_title(title);
        }
    }

    fn prefix_counts(&self, edges: &[u32]) -> io::Result<Vec<u32>> {
        let mut out = vec![0u32; edges.len()];
        let mut next_edge = 0;
        let mut count = 0u32;
        if self.error_total > 0 {
            let mut reader =
                BufReader::with_capacity(ERRORS_BUFFER_BYTES, File::open(&self.errors_path)?);
            let mut record = [0u8; 4];
            loop {
                match reader.read_exact(&mut record) {
                    Ok(()) => {}
                    Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
                    Err(e) => return Err(e),
                }
                let line = u32::from_le_bytes(record);
                while next_edge < edges.len() && edges[next_edge] <= line {
                    out[next_edge] = count;
                    next_edge += 1;
                }
                count += 1;
            }
        }
        for slot in &mut out[next_edge..] {
            *slot = count;
        }
        Ok(out)
    }

    fn nth_error(&self, nth: u32) -> io::Result<Option<u32>> {
        if let Some(&line) = self.error_lines.get(nth as usize) {
            return Ok(Some(line));
        }
        let mut file = File::open(&self.errors_path)?;
        file.seek(SeekFrom::Start(u64::from(nth) * 4))?;
        let mut record = [0u8; 4];
        file.read_exact(&mut record)?;
        Ok(Some(u32::from_le_bytes(record)))
    }
}

#[cfg(test)]
mod tests;
