use std::io::{
    self,
    Write,
};

use super::{
    floor_sec,
    Outline,
    OutlineOutput,
};
use crate::domain::{
    LogSection,
    LogSectionKind,
    PipelineStatus,
};

impl Outline {
    pub(crate) fn finish(&mut self, line_count: u32) -> io::Result<OutlineOutput> {
        if let Some(writer) = self.errors.as_mut() {
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
            let last_ms = if group.end.is_none() {
                self.last_ts
            } else {
                group.last_ms
            };
            let duration_ms = match (group.first_ms, last_ms) {
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

        Ok(OutlineOutput {
            sections,
            error_lines: self.error_lines.clone(),
            warning_lines: self.warning_lines.clone(),
            error_total: self.error_total,
            warning_total: self.warning_total,
            focus_line,
        })
    }

    fn step_drafts(&self, line_count: u32) -> Vec<Draft> {
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
        self.steps
            .iter()
            .enumerate()
            .map(|(i, step)| {
                let duration_ms = match (step.started_ms, step.finished_ms) {
                    (Some(started), Some(finished)) => Some((finished - started).max(0)),
                    _ => None,
                };
                Draft {
                    kind: LogSectionKind::Step,
                    node_id: Some(step.node_id.clone()),
                    title: step.title.clone(),
                    status: Some(step.status.clone()),
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
