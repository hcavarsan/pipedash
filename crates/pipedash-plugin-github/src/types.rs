use chrono::{
    DateTime,
    Utc,
};
pub(crate) use octocrab::models::workflows::Run;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct PendingDeployment {
    pub environment: PendingEnvironment,
    pub current_user_can_approve: bool,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PendingEnvironment {
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Deployment {
    pub environment: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub transient_environment: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ShaRef {
    pub sha: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Stamp {
    pub date: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CommitBody {
    pub message: String,
    pub author: Option<Stamp>,
    pub committer: Option<Stamp>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CommitFile {
    pub filename: String,
    #[serde(default)]
    pub patch: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CommitResponse {
    pub sha: String,
    pub commit: CommitBody,
    #[serde(default)]
    pub parents: Vec<ShaRef>,
    #[serde(default)]
    pub files: Option<Vec<CommitFile>>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Comparison {
    pub ahead_by: u32,
    pub behind_by: u32,
    #[serde(default)]
    pub commits: Vec<ShaRef>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AttestationList {
    #[serde(default)]
    pub attestations: Vec<Attestation>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Attestation {
    #[serde(default)]
    pub bundle: Option<Bundle>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Bundle {
    #[serde(rename = "dsseEnvelope", default)]
    pub dsse_envelope: Option<DsseEnvelope>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DsseEnvelope {
    pub payload: String,
}
