pub mod fetch;
pub mod rendered;
pub mod tagged;

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{
    AtomicBool,
    Ordering,
};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use chrono::{
    DateTime,
    Utc,
};
use reqwest::header::{
    HeaderMap,
    CONTENT_RANGE,
};
use serde::de::DeserializeOwned;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::sync::mpsc;

use crate::error::{
    PluginError,
    PluginResult,
};
use crate::http::{
    check_response,
    map_send_error,
};
use crate::types::PipelineStatus;

pub const LOG_CHUNK_STALL_TIMEOUT: Duration = Duration::from_secs(60);

pub async fn next_log_chunk(response: &mut reqwest::Response) -> PluginResult<Option<Bytes>> {
    match tokio::time::timeout(LOG_CHUNK_STALL_TIMEOUT, response.chunk()).await {
        Err(_) => Err(PluginError::NetworkError(
            "Log download stalled for 60 seconds".into(),
        )),
        Ok(chunk) => chunk.map_err(|e| map_send_error(e, response.url())),
    }
}

const LOG_CHUNK_BYTES: usize = 64 * 1024;

pub const CONTENT_HASH_SEED: u64 = 0xcbf2_9ce4_8422_2325;

pub fn content_hash(bytes: &[u8]) -> u64 {
    extend_content_hash(CONTENT_HASH_SEED, bytes)
}

pub fn extend_content_hash(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogNodeKind {
    Stage,
    Job,
    Step,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogAvailability {
    Ready,
    Partial,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub kind: LogNodeKind,
    pub name: String,
    pub status: PipelineStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub log: Option<LogAvailability>,
    pub web_url: Option<String>,
}

impl LogNode {
    pub fn step(
        job_id: &str, number: impl std::fmt::Display, name: String, status: PipelineStatus,
        started_at: Option<DateTime<Utc>>, finished_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            id: format!("{job_id}:{number}"),
            parent_id: Some(job_id.to_string()),
            kind: LogNodeKind::Step,
            name,
            status,
            started_at,
            finished_at,
            log: None,
            web_url: None,
        }
    }
}

pub struct RunRef<'a> {
    pub pipeline_id: &'a str,
    pub run_number: i64,
    pub run_id: &'a str,
    pub metadata: &'a HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LogCursor(serde_json::Value);

impl LogCursor {
    pub fn new<T: Serialize>(position: &T) -> PluginResult<Self> {
        serde_json::to_value(position)
            .map(Self)
            .map_err(|error| PluginError::SerializationError(error.to_string()))
    }

    pub fn parse<T: DeserializeOwned>(&self) -> Option<T> {
        serde_json::from_value(self.0.clone()).ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogReadMode {
    Append,
    Replace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRead {
    pub mode: LogReadMode,
    pub cursor: Option<LogCursor>,
}

impl LogRead {
    pub fn replace(cursor: Option<LogCursor>) -> Self {
        Self {
            mode: LogReadMode::Replace,
            cursor,
        }
    }

    pub fn append(cursor: LogCursor) -> Self {
        Self {
            mode: LogReadMode::Append,
            cursor: Some(cursor),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineMarker {
    Plain,
    GroupStart,
    GroupEnd,
    SectionStart { open: bool },
    OpenSection,
    Error,
    Warning,
    Notice,
    Debug,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedLine<'a> {
    pub timestamp: Option<DateTime<Utc>>,
    pub marker: LineMarker,
    pub body: &'a [u8],
}

impl<'a> ParsedLine<'a> {
    pub fn plain(line: &'a [u8]) -> Self {
        Self {
            timestamp: None,
            marker: LineMarker::Plain,
            body: line,
        }
    }
}

pub trait LogDialect: Send + Sync {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a>;

    fn is_step_start(&self, _line: &ParsedLine<'_>) -> bool {
        false
    }

    fn infers_errors(&self) -> bool {
        true
    }
}

pub struct PlainDialect;

impl LogDialect for PlainDialect {
    fn parse<'a>(&self, line: &'a [u8]) -> ParsedLine<'a> {
        ParsedLine::plain(line)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentRange {
    pub span: Option<(u64, u64)>,
    pub total: Option<u64>,
}

impl ContentRange {
    pub fn parse(headers: &HeaderMap) -> Option<Self> {
        let value = headers
            .get(CONTENT_RANGE)?
            .to_str()
            .ok()?
            .strip_prefix("bytes ")?;
        let (span, total) = value.split_once('/')?;
        let span = match span.trim() {
            "*" => None,
            span => {
                let (start, end) = span.split_once('-')?;
                Some((start.parse().ok()?, end.parse().ok()?))
            }
        };
        let total = match total.trim() {
            "*" => None,
            total => Some(total.parse().ok()?),
        };
        Some(Self { span, total })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangedRead {
    Appended(u64),
    Replaced(u64),
    Refetch,
}

pub struct LogSink {
    tx: mpsc::Sender<Bytes>,
    bytes: u64,
    restarted: Arc<AtomicBool>,
}

impl LogSink {
    pub fn new(tx: mpsc::Sender<Bytes>) -> Self {
        Self {
            tx,
            bytes: 0,
            restarted: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn restart_signal(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.restarted)
    }

    pub fn restart(&mut self) -> PluginResult<()> {
        if self.bytes > 0 {
            return Err(PluginError::Internal(
                "A log read can only restart before it writes".into(),
            ));
        }
        self.restarted.store(true, Ordering::SeqCst);
        Ok(())
    }

    pub fn restarted(&self) -> bool {
        self.restarted.load(Ordering::SeqCst)
    }

    pub fn bytes_written(&self) -> u64 {
        self.bytes
    }

    pub async fn write_chunks(&mut self, mut bytes: Bytes) -> PluginResult<()> {
        while !bytes.is_empty() {
            self.write(bytes.split_to(bytes.len().min(LOG_CHUNK_BYTES)))
                .await?;
        }
        Ok(())
    }

    pub async fn write(&mut self, chunk: Bytes) -> PluginResult<()> {
        if chunk.is_empty() {
            return Ok(());
        }
        self.bytes += chunk.len() as u64;
        self.tx
            .send(chunk)
            .await
            .map_err(|_| PluginError::Internal("Log reader stopped".into()))
    }

    pub async fn pipe(&mut self, response: reqwest::Response) -> PluginResult<()> {
        self.pipe_after(response, 0).await
    }

    pub async fn pipe_after(
        &mut self, mut response: reqwest::Response, mut skip: u64,
    ) -> PluginResult<()> {
        while let Some(chunk) = next_log_chunk(&mut response).await? {
            let cut = usize::try_from(skip).map_or(chunk.len(), |s| s.min(chunk.len()));
            skip -= cut as u64;
            self.write(chunk.slice(cut..)).await?;
        }
        Ok(())
    }

    pub async fn pipe_range(
        &mut self, response: reqwest::Response, offset: u64,
    ) -> PluginResult<RangedRead> {
        let range = ContentRange::parse(response.headers());
        match response.status().as_u16() {
            206 if range
                .and_then(|r| r.span)
                .is_some_and(|(start, _)| start == offset) =>
            {
                self.pipe(response).await?;
                Ok(RangedRead::Appended(offset + self.bytes))
            }
            416 if range.is_some_and(|r| r.span.is_none() && r.total == Some(offset)) => {
                Ok(RangedRead::Appended(offset))
            }
            200 => {
                self.restart()?;
                self.pipe(response).await?;
                Ok(RangedRead::Replaced(self.bytes))
            }
            206 | 416 => Ok(RangedRead::Refetch),
            _ => {
                check_response(response).await?;
                Ok(RangedRead::Refetch)
            }
        }
    }

    pub async fn pipe_resumed<F, Fut>(
        &mut self, offset: Option<u64>, mut fetch: F,
    ) -> PluginResult<(LogReadMode, u64)>
    where
        F: FnMut(Option<u64>) -> Fut,
        Fut: Future<Output = PluginResult<reqwest::Response>>,
    {
        if let Some(offset) = offset {
            match self.pipe_range(fetch(Some(offset)).await?, offset).await? {
                RangedRead::Appended(next) => return Ok((LogReadMode::Append, next)),
                RangedRead::Replaced(next) => return Ok((LogReadMode::Replace, next)),
                RangedRead::Refetch => self.restart()?,
            }
        }
        self.pipe(check_response(fetch(None).await?).await?).await?;
        Ok((LogReadMode::Replace, self.bytes))
    }
}

#[cfg(test)]
mod tests;
