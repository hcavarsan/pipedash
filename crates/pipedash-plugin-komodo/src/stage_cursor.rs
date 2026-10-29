use pipedash_plugin_api::content_hash;
use serde::{
    Deserialize,
    Serialize,
};

use crate::types::UpdateLog;

fn digest(text: &str) -> u64 {
    content_hash(text.as_bytes())
}

fn grown(new: &str, length: usize, prefix: u64) -> Option<&str> {
    let (old, rest) = (new.get(..length)?, new.get(length..)?);
    if digest(old) != prefix {
        return None;
    }
    if old.is_empty() || old.ends_with('\n') || rest.is_empty() {
        Some(rest)
    } else {
        rest.strip_prefix('\n')
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct StageCursor {
    stage: String,
    start_ts: i64,
    success: bool,
    command: u64,
    stdout_length: usize,
    stdout: u64,
    stderr_length: usize,
    stderr: u64,
}

impl StageCursor {
    pub fn of(log: &UpdateLog) -> Self {
        Self {
            stage: log.stage.clone(),
            start_ts: log.start_ts,
            success: log.success,
            command: digest(&log.command),
            stdout_length: log.stdout.len(),
            stdout: digest(&log.stdout),
            stderr_length: log.stderr.len(),
            stderr: digest(&log.stderr),
        }
    }

    pub fn resume<'a>(&self, log: &'a UpdateLog) -> Option<(&'a str, &'a str)> {
        if log.stage != self.stage
            || log.start_ts != self.start_ts
            || digest(&log.command) != self.command
        {
            return None;
        }
        let stdout = grown(&log.stdout, self.stdout_length, self.stdout)?;
        let stderr = grown(&log.stderr, self.stderr_length, self.stderr)?;
        if self.stderr_length == 0 {
            return Some((stdout, stderr));
        }
        (log.success == self.success && stdout.is_empty()).then_some(("", stderr))
    }
}
