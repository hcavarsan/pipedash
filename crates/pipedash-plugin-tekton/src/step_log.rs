use chrono::{
    DateTime,
    Utc,
};
use k8s_openapi::api::core::v1::Pod;
use pipedash_plugin_api::LogDialect;
use serde::{
    Deserialize,
    Serialize,
};

use crate::log_dialect::TektonLogDialect;
use crate::types::{
    StepState,
    TektonTaskRun,
};

const MAX_PENDING_LINE_BYTES: usize = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Stamp {
    pub at: DateTime<Utc>,
    pub seen: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Instance {
    restarts: i32,
    id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ContainerCursor {
    pub name: String,
    pub instance: Instance,
    pub done: bool,
    pub capped: bool,
    pub last: Option<Stamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TektonCursor {
    pub pod: String,
    pub containers: Vec<ContainerCursor>,
}

pub(crate) struct StartedStep<'a> {
    pub step: &'a StepState,
    pub container: String,
    pub instance: Instance,
    pub done: bool,
}

pub(crate) fn started_steps<'a>(task_run: &'a TektonTaskRun, pod: &Pod) -> Vec<StartedStep<'a>> {
    let statuses = pod
        .status
        .as_ref()
        .and_then(|status| status.container_statuses.as_deref())
        .unwrap_or_default();
    task_run
        .status
        .steps
        .iter()
        .filter(|step| step.has_started())
        .map(|step| {
            let container = step.container_name();
            let instance = statuses
                .iter()
                .find(|status| status.name == container)
                .map(|status| Instance {
                    restarts: status.restart_count,
                    id: status.container_id.clone().unwrap_or_default(),
                })
                .unwrap_or(Instance {
                    restarts: 0,
                    id: String::new(),
                });
            StartedStep {
                step,
                container,
                instance,
                done: step.terminated.is_some(),
            }
        })
        .collect()
}

impl TektonCursor {
    pub(crate) fn continues(&self, pod: &str, steps: &[StartedStep<'_>]) -> bool {
        self.pod == pod
            && self.containers.len() <= steps.len()
            && self
                .containers
                .iter()
                .zip(steps)
                .all(|(seen, step)| seen.name == step.container && seen.instance == step.instance)
            && self.containers.split_last().is_none_or(|(last, earlier)| {
                earlier.iter().all(|seen| seen.done) && (last.done || !last.capped)
            })
    }

    pub(crate) fn snapshot_only(&self) -> bool {
        self.containers.iter().any(|seen| seen.capped && !seen.done)
    }
}

pub(crate) struct Diverged;

pub(crate) struct StepSummary {
    pub lines: usize,
    pub last: Option<Stamp>,
}

struct Skip {
    at: DateTime<Utc>,
    remaining: usize,
}

pub(crate) struct StepFilter {
    skip: Option<Skip>,
    pending: Vec<u8>,
    hold_tail: bool,
    lines: usize,
    last: Option<Stamp>,
}

impl StepFilter {
    pub(crate) fn new(from: Option<Stamp>, hold_tail: bool) -> Self {
        Self {
            skip: from.map(|stamp| Skip {
                at: stamp.at,
                remaining: stamp.seen,
            }),
            pending: Vec::new(),
            hold_tail,
            lines: 0,
            last: from,
        }
    }

    pub(crate) fn push(&mut self, chunk: &[u8], out: &mut Vec<u8>) -> Result<(), Diverged> {
        let mut rest = chunk;
        while let Some(end) = rest.iter().position(|byte| *byte == b'\n') {
            let (line, tail) = rest.split_at(end + 1);
            self.complete(line, out)?;
            rest = tail;
        }
        self.pending.extend_from_slice(rest);
        if self.pending.len() > MAX_PENDING_LINE_BYTES {
            self.complete(b"\n", out)?;
        }
        Ok(())
    }

    pub(crate) fn finish(mut self, out: &mut Vec<u8>) -> Result<StepSummary, Diverged> {
        if !self.hold_tail && !self.pending.is_empty() {
            self.complete(b"\n", out)?;
        }
        if self.skip.as_ref().is_some_and(|skip| skip.remaining > 0) {
            return Err(Diverged);
        }
        Ok(StepSummary {
            lines: self.lines,
            last: self.last,
        })
    }

    fn complete(&mut self, tail: &[u8], out: &mut Vec<u8>) -> Result<(), Diverged> {
        if self.pending.is_empty() {
            return self.line(tail, out);
        }
        let mut line = std::mem::take(&mut self.pending);
        line.extend_from_slice(tail);
        let result = self.line(&line, out);
        line.clear();
        self.pending = line;
        result
    }

    fn line(&mut self, line: &[u8], out: &mut Vec<u8>) -> Result<(), Diverged> {
        let stamp = TektonLogDialect
            .parse(line.strip_suffix(b"\n").unwrap_or(line))
            .timestamp;
        if let Some(skip) = self.skip.as_mut() {
            let stamp = stamp.unwrap_or(skip.at);
            if stamp < skip.at {
                return Ok(());
            }
            if skip.remaining > 0 {
                if stamp > skip.at {
                    return Err(Diverged);
                }
                skip.remaining -= 1;
                return Ok(());
            }
            self.skip = None;
        }
        self.lines += 1;
        if let Some(stamp) = stamp.or(self.last.map(|last| last.at)) {
            match &mut self.last {
                Some(last) if last.at == stamp => last.seen += 1,
                last => *last = Some(Stamp { at: stamp, seen: 1 }),
            }
        }
        out.extend_from_slice(line);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(stamp: &str) -> DateTime<Utc> {
        stamp.parse().unwrap()
    }

    fn run(
        from: Option<Stamp>, hold_tail: bool, chunks: &[&str],
    ) -> Result<(String, StepSummary), ()> {
        let mut filter = StepFilter::new(from, hold_tail);
        let mut out = Vec::new();
        for chunk in chunks {
            filter.push(chunk.as_bytes(), &mut out).map_err(|_| ())?;
        }
        let summary = filter.finish(&mut out).map_err(|_| ())?;
        Ok((String::from_utf8(out).unwrap(), summary))
    }

    const LOG: &str = "2026-10-28T10:00:01.5Z a\n2026-10-28T10:00:02Z b\n2026-10-28T10:00:02Z b\n2026-10-28T10:00:03Z c\n";

    #[test]
    fn the_cursor_stamps_the_last_timestamp_with_its_line_count() {
        let (text, summary) = run(None, false, &[LOG]).unwrap();

        assert_eq!(text, LOG);
        assert_eq!(summary.lines, 4);
        assert_eq!(
            summary.last,
            Some(Stamp {
                at: at("2026-10-28T10:00:03Z"),
                seen: 1
            })
        );
        let (_, summary) = run(
            None,
            false,
            &["2026-10-28T10:00:02Z x\n2026-10-28T10:00:02Z x\n"],
        )
        .unwrap();
        assert_eq!(summary.last.unwrap().seen, 2);
    }

    #[test]
    fn resuming_drops_the_lines_already_seen_at_the_boundary_timestamp() {
        let from = Stamp {
            at: at("2026-10-28T10:00:02Z"),
            seen: 2,
        };
        let more = "2026-10-28T10:00:02Z b\n2026-10-28T10:00:04Z d\n";
        let body = format!(
            "2026-10-28T10:00:01.5Z a\n2026-10-28T10:00:02Z b\n2026-10-28T10:00:02Z b\n{more}"
        );

        let (text, summary) = run(Some(from), false, &[&body]).unwrap();

        assert_eq!(text, more);
        assert_eq!(
            summary.last,
            Some(Stamp {
                at: at("2026-10-28T10:00:04Z"),
                seen: 1
            })
        );
    }

    #[test]
    fn resuming_with_nothing_new_keeps_the_cursor() {
        let from = Stamp {
            at: at("2026-10-28T10:00:03Z"),
            seen: 1,
        };

        let (text, summary) = run(Some(from), false, &["2026-10-28T10:00:03Z c\n"]).unwrap();

        assert_eq!(text, "");
        assert_eq!(summary.last, Some(from));
    }

    #[test]
    fn a_boundary_line_that_vanished_diverges() {
        let from = Stamp {
            at: at("2026-10-28T10:00:02Z"),
            seen: 2,
        };
        let cases: [&[&str]; 3] = [
            &[],
            &["2026-10-28T10:00:02Z b\n2026-10-28T10:00:05Z later\n"],
            &["2026-10-28T10:00:09Z later\n"],
        ];

        for chunks in cases {
            assert!(run(Some(from), false, chunks).is_err(), "{chunks:?}");
        }
    }

    #[test]
    fn splitting_the_stream_anywhere_yields_the_same_bytes() {
        let whole = run(None, false, &[LOG]).unwrap().0;

        for cut in 0..LOG.len() {
            let (head, tail) = LOG.split_at(cut);
            assert_eq!(run(None, false, &[head, tail]).unwrap().0, whole, "{cut}");
        }
    }

    #[test]
    fn an_unterminated_last_line_is_held_back_while_the_container_runs() {
        let body = "2026-10-28T10:00:01Z a\n2026-10-28T10:00:02Z par";

        let (running, summary) = run(None, true, &[body]).unwrap();
        let (finished, _) = run(None, false, &[body]).unwrap();

        assert_eq!(running, "2026-10-28T10:00:01Z a\n");
        assert_eq!(summary.lines, 1);
        assert_eq!(summary.last.unwrap().at, at("2026-10-28T10:00:01Z"));
        assert_eq!(
            finished,
            "2026-10-28T10:00:01Z a\n2026-10-28T10:00:02Z par\n"
        );
    }

    #[test]
    fn an_oversized_unterminated_line_is_cut_into_lines() {
        let big = "x".repeat(MAX_PENDING_LINE_BYTES + 1);
        let body = format!("2026-10-28T10:00:01Z {big}");

        let (text, summary) = run(None, true, &[&body]).unwrap();

        assert_eq!(text, format!("{body}\n"));
        assert_eq!(summary.lines, 1);
    }
}
