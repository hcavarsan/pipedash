use std::collections::HashMap;
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
use crate::http::map_send_error;
use crate::types::PipelineStatus;

pub const LOG_CHUNK_STALL_TIMEOUT: Duration = Duration::from_secs(60);

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

pub struct LogSink {
    tx: mpsc::Sender<Bytes>,
    written: bool,
    restarted: Arc<AtomicBool>,
}

impl LogSink {
    pub fn new(tx: mpsc::Sender<Bytes>) -> Self {
        Self {
            tx,
            written: false,
            restarted: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn restart_signal(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.restarted)
    }

    pub fn restart(&mut self) -> PluginResult<()> {
        if self.written {
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

    pub async fn write(&mut self, chunk: Bytes) -> PluginResult<()> {
        if chunk.is_empty() {
            return Ok(());
        }
        self.written = true;
        self.tx
            .send(chunk)
            .await
            .map_err(|_| PluginError::Internal("Log reader stopped".into()))
    }

    pub async fn pipe(&mut self, mut response: reqwest::Response) -> PluginResult<()> {
        let url = response.url().clone();
        loop {
            match tokio::time::timeout(LOG_CHUNK_STALL_TIMEOUT, response.chunk()).await {
                Err(_) => {
                    return Err(PluginError::NetworkError(
                        "Log download stalled for 60 seconds".into(),
                    ))
                }
                Ok(Err(e)) => return Err(map_send_error(e, &url)),
                Ok(Ok(None)) => return Ok(()),
                Ok(Ok(Some(chunk))) => self.write(chunk).await?,
            }
        }
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

    #[tokio::test]
    async fn pipe_delivers_chunked_bytes_exactly() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                      5\r\nhello\r\n7\r\n world\n\r\n3\r\nend\r\n0\r\n\r\n",
                );
            }
        });
        let response = reqwest::Client::new()
            .get(format!("http://{addr}/log"))
            .send()
            .await
            .unwrap();
        let (tx, mut rx) = mpsc::channel(16);
        let mut sink = LogSink::new(tx);
        sink.pipe(response).await.unwrap();
        drop(sink);
        let mut got = Vec::new();
        while let Some(chunk) = rx.recv().await {
            got.extend_from_slice(&chunk);
        }
        assert_eq!(got, b"hello world\nend");
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
