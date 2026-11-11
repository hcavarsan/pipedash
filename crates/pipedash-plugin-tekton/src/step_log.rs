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
use crate::types::StepState;

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
    pub sidecar: bool,
}

pub(crate) fn started_steps<'a>(
    steps: &'a [StepState], pod: &Pod, sidecar: bool,
) -> Vec<StartedStep<'a>> {
    let status = pod.status.as_ref();
    steps
        .iter()
        .filter(|step| step.has_started())
        .map(|step| {
            let container = step.container_name(if sidecar { "sidecar" } else { "step" });
            let instance = status
                .into_iter()
                .flat_map(|status| {
                    status
                        .container_statuses
                        .iter()
                        .flatten()
                        .chain(status.init_container_statuses.iter().flatten())
                })
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
                sidecar,
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
mod tests;
