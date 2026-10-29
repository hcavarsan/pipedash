use chrono::{
    DateTime,
    Utc,
};
pub use pipedash_plugin_api::{
    LineMarker,
    LogAvailability,
    LogCursor,
    LogDialect,
    LogNodeKind,
    LogRead,
    LogReadMode,
    LogSink,
    ParsedLine,
    PlainDialect,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::pipeline::PipelineStatus;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunLogTree {
    pub nodes: Vec<RunLogNode>,
    pub default_node_id: Option<String>,
    pub complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunLogNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub kind: LogNodeKind,
    pub name: String,
    pub status: PipelineStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i64>,
    pub log: Option<LogAvailability>,
    pub web_url: Option<String>,
    pub depth: u8,
    pub navigable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RunLogDocument {
    Ready(LogDocumentMeta),
    Loading { received_bytes: u64 },
    Pending,
    Unavailable { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogDocumentMeta {
    pub node_id: String,
    pub revision: i64,
    pub complete: bool,
    pub truncated: bool,
    pub line_count: u32,
    pub text_bytes: u64,
    pub first_timestamp_ms: Option<i64>,
    pub styles: Vec<LogStyle>,
    pub sections: Vec<LogSection>,
    pub error_lines: Vec<u32>,
    pub warning_lines: Vec<u32>,
    pub error_total: u32,
    pub warning_total: u32,
    pub focus_line: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogSectionKind {
    Step,
    Group,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSection {
    pub id: u32,
    pub parent: Option<u32>,
    pub kind: LogSectionKind,
    pub node_id: Option<String>,
    pub title: String,
    pub status: Option<PipelineStatus>,
    pub duration_ms: Option<i64>,
    pub header_line: Option<u32>,
    pub start_line: u32,
    pub end_line: u32,
    pub error_count: u32,
    pub open: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct LogStyle {
    pub fg: Option<AnsiColor>,
    pub bg: Option<AnsiColor>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnsiColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum LineKind {
    #[default]
    Plain = 0,
    Group = 1,
    Error = 2,
    Warning = 3,
    Notice = 4,
    Debug = 5,
    Command = 6,
}

impl LineKind {
    pub fn is_plain(&self) -> bool {
        *self == LineKind::Plain
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => LineKind::Plain,
            1 => LineKind::Group,
            2 => LineKind::Error,
            3 => LineKind::Warning,
            4 => LineKind::Notice,
            5 => LineKind::Debug,
            6 => LineKind::Command,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLinesPage {
    pub start: u32,
    pub lines: Vec<LogLine>,
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogLine {
    pub text: String,
    #[serde(default, skip_serializing_if = "LineKind::is_plain")]
    pub kind: LineKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<(u32, u32, u16)>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub omitted: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogSearchQuery {
    pub text: String,
    pub regex: bool,
    pub case_sensitive: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogSearchResult {
    pub matches: Vec<(u32, u32, u32)>,
    pub total: u64,
    pub capped: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct LogCacheStats {
    pub bytes: u64,
    pub runs: u64,
}
