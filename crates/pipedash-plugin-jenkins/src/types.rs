//! API response types for Jenkins API

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct JobItem {
    pub name: String,
    #[serde(rename = "_class")]
    pub _class: String,
}

pub(crate) struct DiscoveredJob {
    pub name: String,
    pub full_path: String,
    pub _class: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Job {
    pub name: String,
    #[serde(rename = "_class")]
    #[serde(default)]
    pub _class: Option<String>,
    #[serde(rename = "lastBuild")]
    #[serde(default)]
    pub last_build: Option<BuildRef>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BuildRef {
    pub number: i64,
    #[serde(default)]
    #[allow(dead_code)]
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Build {
    pub number: i64,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub result: Option<String>,
    #[serde(default)]
    pub building: bool,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub duration: i64,
    #[serde(rename = "fullDisplayName")]
    #[serde(default)]
    #[allow(dead_code)]
    pub full_display_name: Option<String>,
    #[serde(default)]
    pub actions: Vec<BuildAction>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BuildAction {
    #[serde(rename = "_class")]
    #[serde(default)]
    #[allow(dead_code)]
    pub _class: Option<String>,
    #[serde(default)]
    pub causes: Vec<BuildCause>,
    #[serde(rename = "lastBuiltRevision")]
    #[serde(default)]
    pub last_built_revision: Option<Revision>,
    #[serde(default)]
    pub parameters: Vec<BuildParameter>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BuildParameter {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub value: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BuildCause {
    #[serde(rename = "shortDescription")]
    #[serde(default)]
    #[allow(dead_code)]
    pub short_description: Option<String>,
    #[serde(rename = "userName")]
    #[serde(default)]
    pub user_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Revision {
    #[serde(rename = "SHA1")]
    #[serde(default)]
    #[allow(dead_code)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub branch: Vec<Branch>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Branch {
    #[serde(rename = "SHA1")]
    #[serde(default)]
    pub sha1: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct JobProperty {
    #[serde(rename = "_class")]
    #[serde(default)]
    pub _class: Option<String>,
    #[serde(rename = "parameterDefinitions")]
    #[serde(default)]
    pub parameter_definitions: Vec<ParameterDefinition>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ParameterDefinition {
    #[serde(rename = "_class")]
    #[serde(default)]
    pub _class: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "defaultParameterValue")]
    #[serde(default)]
    pub default_parameter_value: Option<DefaultValue>,
    #[serde(default)]
    pub choices: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DefaultValue {
    #[serde(default)]
    pub value: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct JobWithParameters {
    #[serde(default)]
    pub property: Vec<JobProperty>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct JobsResponse {
    #[serde(default)]
    pub jobs: Vec<JobItem>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct JobBuildsResponse {
    pub builds: Vec<Build>,
}
