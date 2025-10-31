//! API response types for Buildkite API
//!
//! These types are internal implementation details for deserializing
//! Buildkite API responses.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Pipeline {
    #[allow(dead_code)]
    pub id: String,
    #[allow(dead_code)]
    pub slug: String,
    pub name: String,
    #[allow(dead_code)]
    pub url: String,
    pub repository: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Build {
    pub id: String,
    pub number: i64,
    pub state: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub url: String,
    pub web_url: String,
    #[serde(default)]
    pub branch: String,
    pub message: Option<String>,
    #[serde(default)]
    pub commit: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub author: Option<Author>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Author {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Agent {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub ip_address: String,
    #[serde(default)]
    pub connected: bool,
    pub job: Option<AgentJob>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AgentJob {
    pub id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Artifact {
    pub id: String,
    pub filename: String,
    pub size: i64,
    #[allow(dead_code)]
    pub url: String,
    #[serde(default)]
    pub download_url: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Organization {
    pub slug: String,
}
