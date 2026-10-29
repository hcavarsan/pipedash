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
        ParsedLine {
            timestamp: None,
            marker: LineMarker::Plain,
            body: line,
        }
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
mod tests {
    use std::io::{
        Read,
        Write,
    };
    use std::net::TcpListener;

    use super::*;

    async fn serve(raw: &'static str) -> reqwest::Response {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(raw.as_bytes());
            }
        });
        reqwest::Client::new()
            .get(format!("http://{addr}/log"))
            .send()
            .await
            .unwrap()
    }

    async fn drain(mut rx: mpsc::Receiver<Bytes>) -> Vec<u8> {
        let mut got = Vec::new();
        while let Some(chunk) = rx.recv().await {
            got.extend_from_slice(&chunk);
        }
        got
    }

    async fn ranged(
        raw: &'static str, offset: u64,
    ) -> (PluginResult<RangedRead>, Vec<u8>, bool, u64) {
        let response = serve(raw).await;
        let (tx, rx) = mpsc::channel(16);
        let mut sink = LogSink::new(tx);
        let outcome = sink.pipe_range(response, offset).await;
        let restarted = sink.restarted();
        let written = sink.bytes_written();
        drop(sink);
        (outcome, drain(rx).await, restarted, written)
    }

    #[tokio::test]
    async fn pipe_delivers_chunked_bytes_exactly() {
        let response = serve(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
             5\r\nhello\r\n7\r\n world\n\r\n3\r\nend\r\n0\r\n\r\n",
        )
        .await;
        let (tx, rx) = mpsc::channel(16);
        let mut sink = LogSink::new(tx);
        sink.pipe(response).await.unwrap();
        assert_eq!(sink.bytes_written(), 15);
        drop(sink);
        assert_eq!(drain(rx).await, b"hello world\nend");
    }

    #[tokio::test]
    async fn pipe_after_drops_the_overlap_across_chunks() {
        let response = serve(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
             2\r\nab\r\n3\r\ncde\r\n1\r\nf\r\n0\r\n\r\n",
        )
        .await;
        let (tx, rx) = mpsc::channel(16);
        let mut sink = LogSink::new(tx);
        sink.pipe_after(response, 4).await.unwrap();
        assert_eq!(sink.bytes_written(), 2);
        drop(sink);
        assert_eq!(drain(rx).await, b"ef");
    }

    #[test]
    fn content_hash_is_fnv1a_64_and_continues_across_chunks() {
        assert_eq!(content_hash(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(content_hash(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(content_hash(b"foobar"), 0x8594_4171_f739_67e8);
        assert_eq!(
            extend_content_hash(content_hash(b"foo"), b"bar"),
            content_hash(b"foobar")
        );
    }

    #[test]
    fn content_range_reads_spans_totals_and_unknowns() {
        let parse = |value: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_RANGE, value.parse().unwrap());
            ContentRange::parse(&headers)
        };
        assert_eq!(
            parse("bytes 5-9/10"),
            Some(ContentRange {
                span: Some((5, 9)),
                total: Some(10)
            })
        );
        assert_eq!(
            parse("bytes 5-9/*"),
            Some(ContentRange {
                span: Some((5, 9)),
                total: None
            })
        );
        assert_eq!(
            parse("bytes */10"),
            Some(ContentRange {
                span: None,
                total: Some(10)
            })
        );
        for malformed in ["items 5-9/10", "bytes 5-9", "bytes a-9/10", "bytes 5-/10"] {
            assert_eq!(parse(malformed), None, "{malformed}");
        }
        assert_eq!(ContentRange::parse(&HeaderMap::new()), None);
    }

    #[tokio::test]
    async fn range_reply_starting_at_the_offset_appends() {
        let (outcome, got, restarted, _) = ranged(
            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 5-9/10\r\nContent-Length: 5\r\n\r\nworld",
            5,
        )
        .await;
        assert_eq!(outcome.unwrap(), RangedRead::Appended(10));
        assert_eq!(got, b"world");
        assert!(!restarted);
    }

    #[tokio::test]
    async fn range_reply_starting_elsewhere_or_unmarked_is_refetched_unwritten() {
        for raw in [
            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 6-9/10\r\nContent-Length: 4\r\n\r\norld",
            "HTTP/1.1 206 Partial Content\r\nContent-Length: 5\r\n\r\nworld",
            "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */3\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 204 No Content\r\n\r\n",
        ] {
            let (outcome, got, restarted, written) = ranged(raw, 5).await;
            assert_eq!(outcome.unwrap(), RangedRead::Refetch, "{raw}");
            assert!(got.is_empty() && !restarted && written == 0, "{raw}");
        }
    }

    #[tokio::test]
    async fn range_not_satisfiable_at_the_exact_total_appends_nothing() {
        let (outcome, got, restarted, _) = ranged(
            "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */5\r\nContent-Length: 0\r\n\r\n",
            5,
        )
        .await;
        assert_eq!(outcome.unwrap(), RangedRead::Appended(5));
        assert!(got.is_empty() && !restarted);
    }

    #[tokio::test]
    async fn full_body_after_a_range_restarts_before_writing() {
        let (outcome, got, restarted, _) =
            ranged("HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nhelloworld", 5).await;
        assert_eq!(outcome.unwrap(), RangedRead::Replaced(10));
        assert_eq!(got, b"helloworld");
        assert!(restarted);
    }

    #[tokio::test]
    async fn range_reply_with_a_server_error_fails() {
        let (outcome, got, restarted, _) = ranged(
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n",
            5,
        )
        .await;
        assert!(matches!(
            outcome,
            Err(PluginError::ServiceUnavailable { .. })
        ));
        assert!(got.is_empty() && !restarted);
    }

    #[tokio::test]
    async fn write_after_reader_dropped_is_internal() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let mut sink = LogSink::new(tx);
        let err = sink.write(Bytes::from_static(b"x")).await.unwrap_err();
        assert!(matches!(err, PluginError::Internal(_)));
    }
}
