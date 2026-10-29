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

use super::{
    MARKER_LINES_CAP,
    MAX_SECTIONS,
};
use crate::domain::{
    LineKind,
    LogSection,
    LogSectionKind,
    PipelineStatus,
};

const ERRORS_FILE: &str = "errors.bin";
const ERRORS_BUFFER_BYTES: usize = 64 * 1024;
const MAX_TITLE_BYTES: usize = 512;
const MAX_STEP_MARKERS: usize = 100_000;

#[derive(Debug, Clone)]
pub struct StepBound {
    pub node_id: String,
    pub title: String,
    pub status: PipelineStatus,
    pub started_ms: Option<i64>,
    pub finished_ms: Option<i64>,
}

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

pub(crate) struct Outline {
    steps: Vec<StepBound>,
    ts_boundary: Vec<u32>,
    markers: Vec<(u32, i64)>,
    groups: Vec<Group>,
    open_groups: Vec<usize>,
    ignored_depth: u32,
    warned: bool,
    last_ts: Option<i64>,
    errors_path: PathBuf,
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

impl Outline {
    pub(crate) fn new(mut steps: Vec<StepBound>, dir: &Path) -> Self {
        steps.sort_by_key(|s| (s.started_ms.is_none(), s.started_ms));
        steps.truncate(MAX_SECTIONS);
        Self {
            steps,
            ts_boundary: Vec::new(),
            markers: Vec::new(),
            groups: Vec::new(),
            open_groups: Vec::new(),
            ignored_depth: 0,
            warned: false,
            last_ts: None,
            errors_path: dir.join(ERRORS_FILE),
            errors: None,
            error_total: 0,
            warning_total: 0,
            error_lines: Vec::new(),
            warning_lines: Vec::new(),
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

    pub(crate) fn finish(mut self, line_count: u32) -> io::Result<OutlineOutput> {
        for &group in &self.open_groups {
            let group = &mut self.groups[group];
            group.end = Some(line_count);
            group.last_ms = self.last_ts;
        }
        if let Some(mut writer) = self.errors.take() {
            writer.flush()?;
        }

        let mut drafts = self.step_drafts(line_count);
        let step_count = drafts.len();
        let ranges: Vec<(u32, u32)> = drafts.iter().map(|d| (d.start, d.end)).collect();
        let mut stack: Vec<(usize, Option<usize>)> = Vec::new();
        for group in &self.groups {
            let step = containing_step(&ranges, group.header);
            while let Some(&(top, top_step)) = stack.last() {
                if drafts[top].end <= group.header || top_step != step {
                    stack.pop();
                } else {
                    break;
                }
            }
            let parent = stack.last().map(|&(top, _)| top).or(step);
            let mut end = group.end.unwrap_or(line_count);
            if let Some(parent) = parent {
                end = end.min(drafts[parent].end);
            }
            let duration_ms = match (group.first_ms, group.last_ms) {
                (Some(first), Some(last)) => Some((last - first).max(0)),
                _ => None,
            };
            drafts.push(Draft {
                kind: LogSectionKind::Group,
                node_id: None,
                title: group.title.clone(),
                status: None,
                duration_ms,
                header: Some(group.header),
                start: group.header + 1,
                end,
                parent,
                depth: parent.map_or(0, |p| drafts[p].depth.saturating_add(1)),
                open: group.open,
            });
            stack.push((drafts.len() - 1, step));
        }

        let mut edges: Vec<u32> = drafts
            .iter()
            .flat_map(|d| [d.edge_start(), d.end])
            .collect();
        edges.sort_unstable();
        edges.dedup();
        let prefix = self.prefix_counts(&edges)?;
        let prefix_at = |edge: u32| {
            edges
                .binary_search(&edge)
                .ok()
                .and_then(|i| prefix.get(i).copied())
                .unwrap_or(0)
        };

        let focus_line = {
            let failed = drafts[..step_count]
                .iter()
                .find(|d| d.status == Some(PipelineStatus::Failed));
            let in_failed = failed.and_then(|d| {
                let (from, to) = (prefix_at(d.edge_start()), prefix_at(d.end));
                (from < to).then_some(from)
            });
            match in_failed {
                Some(nth) => self.nth_error(nth)?,
                None => self
                    .error_lines
                    .first()
                    .copied()
                    .or_else(|| failed.and_then(|d| (d.start < d.end).then(|| d.end - 1))),
            }
        };

        let only_step = step_count == 1;
        let mut order: Vec<usize> = (0..drafts.len()).collect();
        order.sort_by_key(|&i| (drafts[i].edge_start(), drafts[i].depth));
        let mut new_id = vec![0u32; drafts.len()];
        for (position, &old) in order.iter().enumerate() {
            new_id[old] = position as u32;
        }
        let sections = order
            .iter()
            .map(|&old| {
                let draft = &drafts[old];
                let error_count = prefix_at(draft.end) - prefix_at(draft.edge_start());
                let open = match draft.kind {
                    LogSectionKind::Step => {
                        draft.status == Some(PipelineStatus::Failed) || only_step
                    }
                    LogSectionKind::Group => error_count > 0 || draft.open,
                };
                LogSection {
                    id: new_id[old],
                    parent: draft.parent.map(|p| new_id[p]),
                    kind: draft.kind,
                    node_id: draft.node_id.clone(),
                    title: draft.title.clone(),
                    status: draft.status.clone(),
                    duration_ms: draft.duration_ms,
                    header_line: draft.header,
                    start_line: draft.start,
                    end_line: draft.end,
                    error_count,
                    open,
                }
            })
            .collect();

        match fs::remove_file(&self.errors_path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        Ok(OutlineOutput {
            sections,
            error_lines: self.error_lines,
            warning_lines: self.warning_lines,
            error_total: self.error_total,
            warning_total: self.warning_total,
            focus_line,
        })
    }

    fn step_drafts(&mut self, line_count: u32) -> Vec<Draft> {
        let count = self.steps.len();
        let mut placed: Vec<Option<u32>> = vec![None; count];
        let mut previous: Option<u32> = None;
        let mut cursor = 0;
        for (k, step) in self.steps.iter().enumerate() {
            let Some(started) = step.started_ms else {
                continue;
            };
            if step.status == PipelineStatus::Skipped {
                continue;
            }
            if k == 0 {
                placed[0] = Some(0);
                previous = Some(0);
                continue;
            }
            let start_sec = floor_sec(started);
            let end_sec = step.finished_ms.map(floor_sec);
            let mut found = None;
            while let Some(&(line, sec)) = self.markers.get(cursor) {
                if previous.is_some_and(|p| line <= p) || sec < start_sec {
                    cursor += 1;
                    continue;
                }
                if end_sec.is_some_and(|end| sec > end) {
                    break;
                }
                found = Some(line);
                cursor += 1;
                break;
            }
            let boundary = found.or_else(|| {
                let fallback = self.ts_boundary.get(k).copied().unwrap_or(line_count);
                previous.is_none_or(|p| fallback > p).then_some(fallback)
            });
            if let Some(boundary) = boundary {
                placed[k] = Some(boundary);
                previous = Some(boundary);
            }
        }
        let mut next = line_count;
        let mut starts = vec![0u32; count];
        for k in (0..count).rev() {
            starts[k] = placed[k].unwrap_or(next).min(line_count);
            next = starts[k];
        }
        let steps = std::mem::take(&mut self.steps);
        steps
            .into_iter()
            .enumerate()
            .map(|(i, step)| {
                let duration_ms = match (step.started_ms, step.finished_ms) {
                    (Some(started), Some(finished)) => Some((finished - started).max(0)),
                    _ => None,
                };
                Draft {
                    kind: LogSectionKind::Step,
                    node_id: Some(step.node_id),
                    title: step.title,
                    status: Some(step.status),
                    duration_ms,
                    header: None,
                    start: starts[i],
                    end: starts.get(i + 1).copied().unwrap_or(line_count),
                    parent: None,
                    depth: 0,
                    open: false,
                }
            })
            .collect()
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

struct Draft {
    kind: LogSectionKind,
    node_id: Option<String>,
    title: String,
    status: Option<PipelineStatus>,
    duration_ms: Option<i64>,
    header: Option<u32>,
    start: u32,
    end: u32,
    parent: Option<usize>,
    depth: u8,
    open: bool,
}

impl Draft {
    fn edge_start(&self) -> u32 {
        self.header.unwrap_or(self.start)
    }
}

fn containing_step(ranges: &[(u32, u32)], header: u32) -> Option<usize> {
    ranges
        .partition_point(|&(start, _)| start <= header)
        .checked_sub(1)
        .filter(|&i| header < ranges[i].1)
}

#[cfg(test)]
mod tests {
    use chrono::{
        DateTime,
        SecondsFormat,
    };
    use pipedash_plugin_actions::ActionsLogDialect;

    use super::*;
    use crate::domain::{
        LineMarker,
        LogDialect,
    };

    const BASE: i64 = 1_700_000_000_000;

    fn t(sec: i64, ms: i64) -> Option<i64> {
        Some(BASE + sec * 1000 + ms)
    }

    fn step(id: &str, status: PipelineStatus, sec: i64) -> StepBound {
        StepBound {
            node_id: id.into(),
            title: id.into(),
            status,
            started_ms: t(sec, 0),
            finished_ms: t(sec + 1, 0),
        }
    }

    struct Feed {
        outline: Outline,
        lines: u32,
        _dir: tempfile::TempDir,
    }

    impl Feed {
        fn new(steps: Vec<StepBound>) -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            Self {
                outline: Outline::new(steps, dir.path()),
                lines: 0,
                _dir: dir,
            }
        }

        fn line(&mut self, ts: Option<i64>, kind: LineKind, text: &str) {
            self.outline
                .line(self.lines, kind, ts, text, false)
                .expect("line");
            self.lines += 1;
        }

        fn raw(&mut self, line: &str) {
            let parsed = ActionsLogDialect.parse(line.as_bytes());
            let ts = parsed.timestamp.map(|t| t.timestamp_millis());
            if parsed.marker == LineMarker::GroupEnd {
                self.outline.group_end(self.lines, ts);
                return;
            }
            let kind = match parsed.marker {
                LineMarker::GroupStart => LineKind::Group,
                LineMarker::Error => LineKind::Error,
                _ => LineKind::Plain,
            };
            let text = String::from_utf8_lossy(parsed.body).into_owned();
            let step_start = ActionsLogDialect.is_step_start(&parsed);
            self.outline
                .line(self.lines, kind, ts, &text, step_start)
                .expect("line");
            self.lines += 1;
        }

        fn plain(&mut self, ts: Option<i64>) {
            self.line(ts, LineKind::Plain, "x");
        }

        fn group(&mut self, ts: Option<i64>, title: &str) {
            self.line(ts, LineKind::Group, title);
        }

        fn error(&mut self, ts: Option<i64>) {
            self.line(ts, LineKind::Error, "boom");
        }

        fn end(&mut self, ts: Option<i64>) {
            self.outline.group_end(self.lines, ts);
        }

        fn finish(self) -> OutlineOutput {
            self.outline.finish(self.lines).expect("finish")
        }
    }

    fn find<'a>(out: &'a OutlineOutput, title: &str) -> &'a LogSection {
        out.sections
            .iter()
            .find(|s| s.title == title)
            .expect("section")
    }

    fn bound(id: &str, status: PipelineStatus, start: (i64, i64), end: (i64, i64)) -> StepBound {
        StepBound {
            node_id: id.into(),
            title: id.into(),
            status,
            started_ms: t(start.0, start.1),
            finished_ms: t(end.0, end.1),
        }
    }

    fn stamp(sec: i64, ms: i64) -> String {
        DateTime::from_timestamp_millis(BASE + sec * 1000 + ms)
            .expect("timestamp")
            .to_rfc3339_opts(SecondsFormat::Millis, true)
    }

    fn raw(feed: &mut Feed, sec: i64, ms: i64, body: &str) {
        feed.raw(&format!("{} {body}", stamp(sec, ms)));
    }

    #[test]
    fn run_logs_outline_actions_markers_place_steps_in_shared_seconds() {
        let mut feed = Feed::new(vec![
            bound("Set up job", PipelineStatus::Success, (10, 0), (10, 0)),
            bound("Bulk output", PipelineStatus::Success, (10, 0), (37, 0)),
            bound("Fail", PipelineStatus::Failed, (37, 0), (37, 0)),
            bound("Complete job", PipelineStatus::Success, (37, 0), (37, 0)),
        ]);
        raw(&mut feed, 10, 0, "Current runner version");
        raw(&mut feed, 10, 1, "##[group]Runner Image");
        raw(&mut feed, 10, 2, "Image: ubuntu");
        raw(&mut feed, 10, 2, "##[endgroup]");
        raw(&mut feed, 10, 3, "##[group]Secret source");
        raw(&mut feed, 10, 4, "Dependabot");
        raw(&mut feed, 10, 4, "##[endgroup]");
        raw(&mut feed, 10, 5, "Prepare workflow directory");
        raw(&mut feed, 10, 10, "##[group]Run echo \"::group::bulk\"");
        raw(&mut feed, 10, 10, "##[endgroup]");
        raw(&mut feed, 10, 11, "##[group]bulk");
        raw(&mut feed, 37, 0, "line 1");
        raw(&mut feed, 37, 900, "line 2");
        raw(&mut feed, 37, 910, "##[endgroup]");
        raw(&mut feed, 37, 920, "##[group]Run echo \"::error::boom\"");
        raw(&mut feed, 37, 920, "##[endgroup]");
        raw(&mut feed, 37, 930, "##[error]boom");
        raw(&mut feed, 37, 931, "##[error]boom again");
        raw(&mut feed, 37, 940, "Cleaning up orphan processes");
        let out = feed.finish();
        let range = |title: &str| (find(&out, title).start_line, find(&out, title).end_line);
        assert_eq!(range("Set up job"), (0, 6));
        assert_eq!(range("Bulk output"), (6, 10));
        assert_eq!(range("Fail"), (10, 14));
        assert_eq!(range("Complete job"), (14, 14));
        assert_eq!(find(&out, "Fail").error_count, 2);
        assert!(find(&out, "Fail").open);
        assert!(!find(&out, "Bulk output").open);
        assert_eq!(out.focus_line, Some(11));
        let bulk = find(&out, "bulk");
        assert_eq!(bulk.parent, Some(find(&out, "Bulk output").id));
        assert_eq!((bulk.header_line, bulk.end_line), (Some(7), 10));
        assert_eq!(
            find(&out, "Runner Image").parent,
            Some(find(&out, "Set up job").id)
        );
    }

    #[test]
    fn run_logs_outline_plain_dialect_falls_back_to_timestamps_first_tied_step_wins() {
        let mut feed = Feed::new(vec![
            bound("pre", PipelineStatus::Success, (1, 0), (2, 0)),
            bound("a", PipelineStatus::Success, (5, 0), (5, 0)),
            bound("b", PipelineStatus::Success, (5, 0), (5, 0)),
        ]);
        feed.plain(t(1, 0));
        feed.plain(t(3, 0));
        feed.plain(t(5, 100));
        feed.plain(t(5, 200));
        let out = feed.finish();
        let range = |title: &str| (find(&out, title).start_line, find(&out, title).end_line);
        assert_eq!(range("pre"), (0, 2));
        assert_eq!(range("a"), (2, 4));
        assert_eq!(range("b"), (4, 4));
    }

    #[test]
    fn run_logs_outline_skipped_step_does_not_steal_the_next_marker() {
        let mut feed = Feed::new(vec![
            bound("first", PipelineStatus::Success, (1, 0), (1, 0)),
            bound("skipped", PipelineStatus::Skipped, (5, 0), (5, 0)),
            bound("real", PipelineStatus::Success, (5, 0), (6, 0)),
        ]);
        raw(&mut feed, 1, 0, "setup");
        raw(&mut feed, 5, 100, "##[group]Run make");
        raw(&mut feed, 5, 200, "##[endgroup]");
        raw(&mut feed, 5, 300, "output");
        let out = feed.finish();
        let range = |title: &str| (find(&out, title).start_line, find(&out, title).end_line);
        assert_eq!(range("first"), (0, 1));
        assert_eq!(range("skipped"), (1, 1));
        assert_eq!(range("real"), (1, 3));
    }

    #[test]
    fn run_logs_outline_markers_after_a_steps_finish_second_are_not_taken() {
        let mut feed = Feed::new(vec![
            bound("s0", PipelineStatus::Success, (1, 0), (1, 0)),
            bound("a", PipelineStatus::Success, (2, 0), (2, 0)),
            bound("b", PipelineStatus::Success, (3, 0), (4, 0)),
        ]);
        raw(&mut feed, 1, 0, "setup");
        raw(&mut feed, 2, 100, "a output");
        raw(&mut feed, 4, 500, "##[group]Run x");
        raw(&mut feed, 4, 600, "##[endgroup]");
        raw(&mut feed, 4, 700, "b output");
        let out = feed.finish();
        let range = |title: &str| (find(&out, title).start_line, find(&out, title).end_line);
        assert_eq!(range("s0"), (0, 1));
        assert_eq!(range("a"), (1, 2));
        assert_eq!(range("b"), (2, 4));
    }

    #[test]
    fn run_logs_outline_unreached_and_unstarted_steps_are_empty_at_the_end() {
        let mut feed = Feed::new(vec![
            StepBound {
                started_ms: None,
                ..step("none", PipelineStatus::Pending, 0)
            },
            step("late", PipelineStatus::Pending, 50),
            step("a", PipelineStatus::Success, 5),
        ]);
        feed.plain(t(5, 0));
        feed.plain(t(6, 0));
        let out = feed.finish();
        assert_eq!(
            (find(&out, "a").start_line, find(&out, "a").end_line),
            (0, 2)
        );
        assert_eq!(
            (find(&out, "late").start_line, find(&out, "late").end_line),
            (2, 2)
        );
        assert_eq!(
            (find(&out, "none").start_line, find(&out, "none").end_line),
            (2, 2)
        );
    }

    #[test]
    fn run_logs_outline_failed_step_is_open_and_success_is_closed() {
        let mut feed = Feed::new(vec![
            step("ok", PipelineStatus::Success, 1),
            step("bad", PipelineStatus::Failed, 2),
        ]);
        feed.plain(t(1, 0));
        feed.plain(t(2, 0));
        let out = feed.finish();
        assert!(!find(&out, "ok").open);
        assert!(find(&out, "bad").open);

        let mut feed = Feed::new(vec![step("only", PipelineStatus::Success, 1)]);
        feed.plain(t(1, 0));
        assert!(feed.finish().sections[0].open);
    }

    #[test]
    fn run_logs_outline_focus_is_first_error_of_failed_step() {
        let mut feed = Feed::new(vec![
            step("early", PipelineStatus::Success, 1),
            step("bad", PipelineStatus::Failed, 2),
        ]);
        for _ in 0..20_000 {
            feed.error(t(1, 0));
        }
        feed.plain(t(2, 0));
        feed.error(t(2, 1));
        feed.error(t(2, 2));
        let out = feed.finish();
        assert_eq!(out.focus_line, Some(20_001));
        assert_eq!(out.error_total, 20_002);
        assert_eq!(out.error_lines.len(), MARKER_LINES_CAP);
        assert_eq!(find(&out, "early").error_count, 20_000);
        assert_eq!(find(&out, "bad").error_count, 2);
    }

    #[test]
    fn run_logs_outline_focus_falls_back_to_first_error() {
        let mut feed = Feed::new(vec![
            step("bad", PipelineStatus::Failed, 1),
            step("ok", PipelineStatus::Success, 2),
        ]);
        feed.plain(t(1, 0));
        feed.plain(t(2, 0));
        feed.error(t(2, 1));
        assert_eq!(feed.finish().focus_line, Some(2));

        let mut feed = Feed::new(vec![]);
        feed.plain(None);
        let out = feed.finish();
        assert_eq!(out.focus_line, None);
        assert!(out.sections.is_empty());
    }

    #[test]
    fn run_logs_outline_focus_lands_on_the_end_of_a_failed_step_without_error_lines() {
        let mut feed = Feed::new(vec![
            step("ok", PipelineStatus::Success, 1),
            step("bad", PipelineStatus::Failed, 2),
            step("after", PipelineStatus::Success, 3),
        ]);
        feed.plain(t(1, 0));
        feed.plain(t(2, 0));
        feed.plain(t(2, 1));
        feed.plain(t(3, 0));
        let out = feed.finish();
        assert_eq!(out.error_total, 0);
        assert_eq!(out.focus_line, Some(2));
    }

    #[test]
    fn run_logs_outline_counts_errors_beyond_the_listed_cap() {
        let mut feed = Feed::new(vec![
            step("one", PipelineStatus::Success, 1),
            step("two", PipelineStatus::Success, 2),
        ]);
        for _ in 0..MARKER_LINES_CAP {
            feed.error(t(1, 0));
        }
        feed.plain(t(2, 0));
        feed.group(t(2, 1), "late group");
        for _ in 0..5 {
            feed.error(t(2, 2));
        }
        feed.end(t(2, 3));
        let out = feed.finish();
        let group = find(&out, "late group");
        assert_eq!(group.error_count, 5);
        assert!(group.open);
        assert_eq!(out.error_total as usize, MARKER_LINES_CAP + 5);
        assert_eq!(out.error_lines.len(), MARKER_LINES_CAP);
    }

    #[test]
    fn run_logs_outline_group_crossing_step_end_is_clamped_and_reparented() {
        let mut feed = Feed::new(vec![
            step("a", PipelineStatus::Success, 1),
            step("b", PipelineStatus::Success, 2),
        ]);
        feed.group(t(1, 0), "outer");
        feed.plain(t(1, 5));
        feed.group(t(1, 6), "inner");
        feed.plain(t(2, 0));
        feed.group(t(2, 1), "third");
        feed.plain(t(2, 2));
        let out = feed.finish();
        let a = find(&out, "a");
        let b = find(&out, "b");
        let outer = find(&out, "outer");
        let inner = find(&out, "inner");
        let third = find(&out, "third");
        assert_eq!((a.end_line, b.start_line), (3, 3));
        assert_eq!(outer.parent, Some(a.id));
        assert_eq!(outer.end_line, 3);
        assert_eq!(inner.parent, Some(outer.id));
        assert_eq!(inner.end_line, 3);
        assert_eq!(third.parent, Some(b.id));
        assert_eq!(third.end_line, 6);
        for (i, section) in out.sections.iter().enumerate() {
            assert_eq!(section.id as usize, i);
            assert!(section.parent.is_none_or(|p| p < section.id));
        }
    }

    #[test]
    fn run_logs_outline_group_duration_uses_first_and_last_timestamps() {
        let mut feed = Feed::new(vec![]);
        feed.group(t(1, 0), "g");
        feed.plain(t(2, 0));
        feed.end(t(3, 500));
        let out = feed.finish();
        assert_eq!(find(&out, "g").duration_ms, Some(2500));
    }

    #[test]
    fn run_logs_outline_caps_sections_at_max() {
        let steps = (0..25_000)
            .map(|i| step(&format!("s{i}"), PipelineStatus::Success, i))
            .collect();
        let out = Feed::new(steps).finish();
        assert_eq!(out.sections.len(), MAX_SECTIONS);
    }

    #[test]
    fn run_logs_outline_ignored_group_ends_do_not_close_tracked_groups() {
        let steps = (0..MAX_SECTIONS - 2)
            .map(|i| step(&format!("s{i}"), PipelineStatus::Success, i as i64))
            .collect();
        let mut feed = Feed::new(steps);
        feed.group(None, "A");
        feed.group(None, "B");
        feed.group(None, "C");
        feed.group(None, "D");
        feed.end(None);
        feed.end(None);
        feed.end(None);
        feed.plain(None);
        feed.end(None);
        let out = feed.finish();
        assert_eq!(out.sections.len(), MAX_SECTIONS);
        let a = find(&out, "A");
        let b = find(&out, "B");
        assert_eq!((a.header_line, a.end_line), (Some(0), 5));
        assert_eq!((b.header_line, b.end_line), (Some(1), 4));
        assert_eq!(b.parent, Some(a.id));
    }
}
