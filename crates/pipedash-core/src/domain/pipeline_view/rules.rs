use std::collections::HashSet;

use serde::{
    Deserialize,
    Serialize,
};

use crate::domain::error::{
    DomainError,
    DomainResult,
};
use crate::domain::pipeline::{
    Pipeline,
    PipelineStatus,
};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewRules {
    pub provider_ids: Vec<i64>,
    pub repositories: Vec<String>,
    pub name_contains: Option<String>,
    pub statuses: Vec<PipelineStatus>,
}

impl ViewRules {
    pub fn is_empty(&self) -> bool {
        self.provider_ids.is_empty()
            && self.repositories.is_empty()
            && self.name_contains.as_deref().is_none_or(str::is_empty)
            && self.statuses.is_empty()
    }

    pub fn validate(&self, known_provider_ids: &HashSet<i64>) -> DomainResult<ViewRules> {
        let mut repositories = Vec::new();
        for pattern in &self.repositories {
            let pattern = pattern.trim();
            if pattern.is_empty() || repositories.iter().any(|p| p == pattern) {
                continue;
            }
            glob::Pattern::new(pattern).map_err(|err| {
                DomainError::InvalidConfig(format!(
                    "Invalid repository pattern \"{pattern}\": {err}"
                ))
            })?;
            repositories.push(pattern.to_string());
        }

        let mut provider_ids = Vec::new();
        for id in &self.provider_ids {
            if !known_provider_ids.contains(id) {
                return Err(DomainError::InvalidConfig(format!(
                    "Unknown provider id {id}"
                )));
            }
            if !provider_ids.contains(id) {
                provider_ids.push(*id);
            }
        }

        let mut statuses = Vec::new();
        for status in &self.statuses {
            if !statuses.contains(status) {
                statuses.push(status.clone());
            }
        }

        let name_contains = self
            .name_contains
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        Ok(ViewRules {
            provider_ids,
            repositories,
            name_contains,
            statuses,
        })
    }

    pub fn matcher(&self) -> RuleMatcher<'_> {
        RuleMatcher {
            rules: self,
            patterns: self
                .repositories
                .iter()
                .filter_map(|pattern| glob::Pattern::new(pattern).ok())
                .collect(),
            needle: self
                .name_contains
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(str::to_lowercase),
            empty: self.is_empty(),
        }
    }
}

pub struct RuleMatcher<'a> {
    rules: &'a ViewRules,
    patterns: Vec<glob::Pattern>,
    needle: Option<String>,
    empty: bool,
}

impl RuleMatcher<'_> {
    pub fn matches(&self, pipeline: &Pipeline) -> bool {
        if self.empty {
            return false;
        }
        let rules = self.rules;
        if !rules.provider_ids.is_empty() && !rules.provider_ids.contains(&pipeline.provider_id) {
            return false;
        }
        if !rules.repositories.is_empty() {
            let options = glob::MatchOptions {
                case_sensitive: false,
                require_literal_separator: false,
                require_literal_leading_dot: false,
            };
            if !self
                .patterns
                .iter()
                .any(|p| p.matches_with(&pipeline.repository, options))
            {
                return false;
            }
        }
        if let Some(needle) = &self.needle {
            if !pipeline.name.to_lowercase().contains(needle.as_str()) {
                return false;
            }
        }
        rules.statuses.is_empty() || rules.statuses.contains(&pipeline.status)
    }
}
