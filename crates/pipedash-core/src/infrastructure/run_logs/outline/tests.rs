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

    fn finish(mut self) -> OutlineOutput {
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
