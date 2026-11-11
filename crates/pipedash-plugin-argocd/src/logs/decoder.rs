use pipedash_plugin_api::logs::tagged::write_line;
use pipedash_plugin_api::{
    from_status,
    LineMarker,
    PluginError,
    PluginResult,
};

use crate::types::logs::PodLogMessage;

const MAX_STREAM_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Default)]
pub(crate) struct PodLogDecoder {
    pending: Vec<u8>,
    pub(crate) lines: usize,
    pub(crate) done: bool,
}

impl PodLogDecoder {
    pub(crate) fn feed(&mut self, chunk: &[u8], out: &mut Vec<u8>) -> PluginResult<()> {
        let Some(newline) = chunk.iter().rposition(|b| *b == b'\n') else {
            self.pending.extend_from_slice(chunk);
            return self.check_pending();
        };
        self.pending.extend_from_slice(&chunk[..=newline]);
        let complete = std::mem::replace(&mut self.pending, chunk[newline + 1..].to_vec());
        complete
            .split(|b| *b == b'\n')
            .try_for_each(|message| self.message(message, out))?;
        self.check_pending()
    }

    fn check_pending(&self) -> PluginResult<()> {
        if self.pending.len() > MAX_STREAM_MESSAGE_BYTES {
            return Err(PluginError::ApiError(format!(
                "ArgoCD sent more than {} MiB of pod logs without a line break",
                MAX_STREAM_MESSAGE_BYTES / (1024 * 1024)
            )));
        }
        Ok(())
    }

    pub(crate) fn finish(&mut self, out: &mut Vec<u8>) -> PluginResult<()> {
        let rest = std::mem::take(&mut self.pending);
        self.message(&rest, out)
    }

    fn message(&mut self, raw: &[u8], out: &mut Vec<u8>) -> PluginResult<()> {
        let raw = raw.trim_ascii();
        if raw.is_empty() || self.done {
            return Ok(());
        }
        let message: PodLogMessage = serde_json::from_slice(raw).map_err(|e| {
            PluginError::ApiError(format!("Failed to parse ArgoCD log stream: {e}"))
        })?;
        if let Some(error) = message.error {
            return Err(from_status(error.http_code, error.message, None));
        }
        let Some(entry) = message.result else {
            return Ok(());
        };
        if entry.last {
            self.done = true;
            if entry.content.is_empty() {
                return Ok(());
            }
        }
        let stamp = entry
            .time_stamp_str
            .as_deref()
            .filter(|stamp| !stamp.starts_with("0001-"));
        let marker = if stamp.is_some() {
            LineMarker::Plain
        } else {
            LineMarker::Warning
        };
        write_line(out, marker, stamp, &entry.content);
        self.lines += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
