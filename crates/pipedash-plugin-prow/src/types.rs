use std::collections::HashMap;

use chrono::{
    DateTime,
    Utc,
};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ProwJobList {
    #[serde(default)]
    pub items: Vec<ProwJob>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ProwJob {
    #[serde(default)]
    pub metadata: ObjectMeta,
    #[serde(default)]
    pub spec: ProwJobSpec,
    #[serde(default)]
    pub status: ProwJobStatus,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ObjectMeta {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub namespace: String,
    #[serde(default, rename = "creationTimestamp")]
    pub creation_timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ProwJobSpec {
    #[serde(default, rename = "type")]
    pub job_type: String,
    #[serde(default)]
    pub cluster: String,
    #[serde(default)]
    pub job: String,
    #[serde(default)]
    pub refs: Option<Refs>,
    #[serde(default)]
    pub extra_refs: Vec<Refs>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct Refs {
    #[serde(default)]
    pub org: String,
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub base_ref: String,
    #[serde(default)]
    pub base_sha: String,
    #[serde(default)]
    pub pulls: Vec<Pull>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct Pull {
    #[serde(default)]
    pub number: i64,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub sha: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub link: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ProwJobStatus {
    #[serde(default, rename = "startTime")]
    pub start_time: Option<DateTime<Utc>>,
    #[serde(default, rename = "completionTime")]
    pub completion_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub build_id: String,
}

const JAVASCRIPT_SAFE_LIMIT: i64 = 1 << 53;
const SNOWFLAKE_TIME_SHIFT: u32 = 22;
const SNOWFLAKE_NODE_SHIFT: u32 = 12;
const SNOWFLAKE_STEP_MASK: i64 = 0xFFF;

pub(crate) fn run_number_of(build_id: i64) -> i64 {
    if build_id < JAVASCRIPT_SAFE_LIMIT {
        return build_id;
    }
    ((build_id >> SNOWFLAKE_TIME_SHIFT) << SNOWFLAKE_NODE_SHIFT) | (build_id & SNOWFLAKE_STEP_MASK)
}

impl ProwJob {
    pub(crate) fn run_number(&self) -> Option<i64> {
        self.status
            .build_id
            .parse::<i64>()
            .ok()
            .filter(|number| *number > 0)
            .map(run_number_of)
    }

    pub(crate) fn primary_refs(&self) -> Option<&Refs> {
        self.spec.refs.as_ref().or(self.spec.extra_refs.first())
    }

    pub(crate) fn is_active(&self) -> bool {
        matches!(
            self.status.state.as_str(),
            "scheduling" | "triggered" | "pending"
        )
    }

    pub(crate) fn namespace(&self) -> &str {
        if self.metadata.namespace.is_empty() {
            "default"
        } else {
            &self.metadata.namespace
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct Started {
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub repos: HashMap<String, String>,
    #[serde(default, rename = "repo-commit")]
    pub repo_commit: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct Finished {
    #[serde(default)]
    pub timestamp: Option<i64>,
    #[serde(default)]
    pub passed: Option<bool>,
    #[serde(default)]
    pub result: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ObjectList {
    #[serde(default)]
    pub prefixes: Vec<String>,
    #[serde(default)]
    pub items: Vec<ObjectItem>,
    #[serde(default, rename = "nextPageToken")]
    pub next_page_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ObjectItem {
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_numbers_are_exact_javascript_numbers_in_build_order() {
        let older = 2_116_011_604_377_079_808_i64;
        let newer = 2_116_011_604_377_079_811_i64;
        let next_ms = 2_116_011_604_381_274_112_i64;

        for build_id in [older, newer, next_ms] {
            assert!(run_number_of(build_id) < JAVASCRIPT_SAFE_LIMIT);
        }
        assert!(run_number_of(older) < run_number_of(newer));
        assert!(run_number_of(newer) < run_number_of(next_ms));

        for sequential in [4_521, 4_294_967_296, JAVASCRIPT_SAFE_LIMIT - 1] {
            assert_eq!(run_number_of(sequential), sequential);
        }
    }
}
