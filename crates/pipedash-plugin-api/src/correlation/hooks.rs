use std::fmt;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::ids::CommitSha;
use super::SourceObservation;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommitInfo {
    pub sha: CommitSha,
    pub authored_at: DateTime<Utc>,
    pub committed_at: DateTime<Utc>,
    pub message: String,
    pub parents: Vec<CommitSha>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommitComparison {
    pub ahead: u32,
    pub behind: u32,
    pub commits: Vec<CommitSha>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChangedFile {
    pub path: String,
    pub patch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Provenance {
    pub pipeline_id: Option<String>,
    pub run_number: Option<i64>,
    pub source: SourceObservation,
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct RegistryAuth {
    pub username: Option<String>,
    pub secret: String,
    pub bearer: bool,
}

impl fmt::Debug for RegistryAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegistryAuth")
            .field("username", &self.username)
            .field("secret", &"<redacted>")
            .field("bearer", &self.bearer)
            .finish()
    }
}
