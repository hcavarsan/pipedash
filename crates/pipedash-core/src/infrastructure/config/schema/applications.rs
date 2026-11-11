use indexmap::IndexMap;
use serde::{
    Deserialize,
    Serialize,
};

use super::default_true;
use crate::domain::application::Stage;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrelationFileConfig {
    #[serde(default = "default_true")]
    pub log_evidence: bool,
    #[serde(default = "default_true")]
    pub registry_read: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignore: Vec<PipelineSelector>,
}

impl Default for CorrelationFileConfig {
    fn default() -> Self {
        Self {
            log_evidence: true,
            registry_read: true,
            ignore: Vec::new(),
        }
    }
}

impl CorrelationFileConfig {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PipelineSelector {
    pub provider: String,
    pub pipeline: String,
}

impl PipelineSelector {
    pub fn matches(&self, provider: &str, local_pipeline_id: &str) -> bool {
        self.provider == provider && glob_matches(&self.pipeline, local_pipeline_id)
    }
}

pub fn glob_matches(pattern: &str, value: &str) -> bool {
    let options = glob::MatchOptions {
        case_sensitive: true,
        require_literal_separator: false,
        require_literal_leading_dot: false,
    };
    glob::Pattern::new(pattern).is_ok_and(|pattern| pattern.matches_with(value, options))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationFileConfig {
    pub name: String,
    #[serde(default)]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<MemberFileConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<PipelineSelector>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub targets: IndexMap<String, TargetFileConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberFileConfig {
    pub provider: String,
    pub pipeline: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<Stage>,
}

impl MemberFileConfig {
    pub fn matches(&self, provider: &str, local_pipeline_id: &str) -> bool {
        self.provider == provider && glob_matches(&self.pipeline, local_pipeline_id)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetFileConfig {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clusters: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub namespaces: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub after: Vec<String>,
}
