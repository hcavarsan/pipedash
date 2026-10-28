use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
};

use super::error::{
    DomainError,
    DomainResult,
};
use super::pipeline::{
    Pipeline,
    PipelineStatus,
};

fn set_or_clear<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

fn status_list<'de, D>(deserializer: D) -> Result<Vec<PipelineStatus>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        List(Vec<PipelineStatus>),
        Joined(String),
    }

    match Raw::deserialize(deserializer)? {
        Raw::List(statuses) => Ok(statuses),
        Raw::Joined(joined) => joined
            .split(',')
            .filter(|status| !status.is_empty())
            .map(|status| {
                PipelineStatus::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(
                    status,
                ))
            })
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum ViewRef {
    #[default]
    All,
    Pinned,
    Custom(i64),
    Config(String),
}

impl fmt::Display for ViewRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ViewRef::All => f.write_str("all"),
            ViewRef::Pinned => f.write_str("pinned"),
            ViewRef::Custom(id) => write!(f, "{id}"),
            ViewRef::Config(key) => write!(f, "config:{key}"),
        }
    }
}

impl FromStr for ViewRef {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let unknown = || DomainError::InvalidConfig(format!("Unknown pipeline view id: {s}"));
        match s {
            "all" => Ok(ViewRef::All),
            "pinned" => Ok(ViewRef::Pinned),
            _ => {
                if let Some(key) = s.strip_prefix("config:") {
                    if key.is_empty() {
                        return Err(unknown());
                    }
                    return Ok(ViewRef::Config(key.to_string()));
                }
                if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
                    return s.parse::<i64>().map(ViewRef::Custom).map_err(|_| unknown());
                }
                Err(unknown())
            }
        }
    }
}

impl TryFrom<String> for ViewRef {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<ViewRef> for String {
    fn from(value: ViewRef) -> Self {
        value.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewKind {
    All,
    Pinned,
    Hidden,
    Custom,
    Config,
}

impl ViewKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ViewKind::All => "all",
            ViewKind::Pinned => "pinned",
            ViewKind::Hidden => "hidden",
            ViewKind::Custom => "custom",
            ViewKind::Config => "config",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortKey {
    #[default]
    Name,
    LastRun,
    Status,
    Organization,
    Provider,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionMode {
    #[default]
    Repository,
    Organization,
    Provider,
    Status,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    Repository,
    Organization,
    Provider,
    Status,
    None,
    View,
}

impl From<SectionMode> for SectionKind {
    fn from(mode: SectionMode) -> Self {
        match mode {
            SectionMode::Repository => SectionKind::Repository,
            SectionMode::Organization => SectionKind::Organization,
            SectionMode::Provider => SectionKind::Provider,
            SectionMode::Status => SectionKind::Status,
            SectionMode::None => SectionKind::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ViewLayout {
    #[default]
    Table,
    Cards,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewDisplay {
    pub sort: SortKey,
    pub direction: SortDirection,
    pub sections: SectionMode,
    pub layout: ViewLayout,
}

impl ViewDisplay {
    pub fn pinned_default() -> Self {
        Self {
            sort: SortKey::Manual,
            direction: SortDirection::Asc,
            sections: SectionMode::None,
            layout: ViewLayout::Cards,
        }
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewCapabilities {
    pub rename: bool,
    pub delete: bool,
    pub edit_rules: bool,
    pub edit_members: bool,
    pub edit_display: bool,
    pub reorder: bool,
}

impl ViewCapabilities {
    pub fn for_kind(kind: ViewKind) -> Self {
        let custom = kind == ViewKind::Custom;
        let pinned = kind == ViewKind::Pinned;
        Self {
            rename: custom,
            delete: custom,
            edit_rules: custom,
            edit_members: custom || pinned,
            edit_display: matches!(kind, ViewKind::All | ViewKind::Pinned | ViewKind::Custom),
            reorder: custom || pinned,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineView {
    pub id: ViewRef,
    pub kind: ViewKind,
    pub name: String,
    pub rules: ViewRules,
    pub display: ViewDisplay,
    pub parent: Option<ViewRef>,
    pub capabilities: ViewCapabilities,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MembershipSource {
    Manual,
    Rule,
    SubView,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ViewMembership {
    pub view: ViewRef,
    pub source: MembershipSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewPipeline {
    #[serde(flatten)]
    pub pipeline: Pipeline,
    pub organization: String,
    pub pinned: bool,
    pub hidden: bool,
    pub memberships: Vec<ViewMembership>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewSection {
    pub id: String,
    pub kind: SectionKind,
    pub title: String,
    pub organization: Option<String>,
    pub provider_ids: Vec<i64>,
    pub status: Option<PipelineStatus>,
    pub status_pipeline_id: Option<String>,
    pub last_run: Option<DateTime<Utc>>,
    pub pending_fetch: bool,
    pub pipelines: Vec<ViewPipeline>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PinKind {
    View,
    Repository,
}

impl PinKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PinKind::View => "view",
            PinKind::Repository => "repository",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PinnedItem {
    pub kind: PinKind,
    #[serde(rename = "ref")]
    pub reference: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedBlock {
    pub kind: PinKind,
    #[serde(rename = "ref")]
    pub reference: String,
    pub name: String,
    pub pipelines: Vec<ViewPipeline>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ViewFacets {
    pub organizations: Vec<String>,
    pub provider_ids: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineViewPage {
    pub view: PipelineView,
    pub pinned: Vec<ViewPipeline>,
    pub pinned_blocks: Vec<PinnedBlock>,
    pub sections: Vec<ViewSection>,
    pub total: usize,
    pub hidden_count: usize,
    pub filters_active: bool,
    pub facets: ViewFacets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateRange {
    #[serde(rename = "today")]
    Today,
    #[serde(rename = "24h")]
    Day,
    #[serde(rename = "7d")]
    Week,
    #[serde(rename = "30d")]
    Month,
    #[serde(rename = "60d")]
    TwoMonths,
    #[serde(rename = "90d")]
    Quarter,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PipelineViewQuery {
    #[serde(default)]
    pub view: ViewRef,
    #[serde(default)]
    pub provider_id: Option<i64>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default, deserialize_with = "status_list")]
    pub statuses: Vec<PipelineStatus>,
    #[serde(default)]
    pub organization: Option<String>,
    #[serde(default)]
    pub date_range: Option<DateRange>,
    #[serde(default)]
    pub show_hidden: bool,
    #[serde(default)]
    pub utc_offset_minutes: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateViewInput {
    pub name: String,
    #[serde(default)]
    pub rules: ViewRules,
    #[serde(default)]
    pub display: ViewDisplay,
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub parent: Option<ViewRef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ViewPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub rules: Option<ViewRules>,
    #[serde(default)]
    pub display: Option<ViewDisplay>,
    #[serde(default)]
    pub members: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "set_or_clear",
        skip_serializing_if = "Option::is_none"
    )]
    pub parent: Option<Option<ViewRef>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewMember {
    pub pipeline_id: String,
    pub name: Option<String>,
    pub repository: Option<String>,
    pub provider_id: i64,
    pub available: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LegacyMarks {
    #[serde(default)]
    pub pinned: Vec<String>,
    #[serde(default)]
    pub hidden: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyImportResult {
    pub imported: usize,
    pub skipped: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_parent_distinguishes_absent_null_and_value() {
        let absent: ViewPatch = serde_json::from_str("{}").unwrap();
        let top: ViewPatch = serde_json::from_str(r#"{"parent":null}"#).unwrap();
        let nested: ViewPatch = serde_json::from_str(r#"{"parent":"12"}"#).unwrap();
        assert_eq!(absent.parent, None);
        assert_eq!(top.parent, Some(None));
        assert_eq!(nested.parent, Some(Some(ViewRef::Custom(12))));
    }

    #[test]
    fn query_statuses_accept_a_list_or_comma_separated_text() {
        let listed: PipelineViewQuery =
            serde_json::from_str(r#"{"statuses":["running","pending"]}"#).unwrap();
        let joined: PipelineViewQuery =
            serde_json::from_str(r#"{"statuses":"running,pending"}"#).unwrap();
        let absent: PipelineViewQuery = serde_json::from_str("{}").unwrap();
        let expected = vec![PipelineStatus::Running, PipelineStatus::Pending];
        assert_eq!(listed.statuses, expected);
        assert_eq!(joined.statuses, expected);
        assert!(absent.statuses.is_empty());
        assert!(
            serde_json::from_str::<PipelineViewQuery>(r#"{"statuses":"running,bogus"}"#).is_err()
        );
    }
}
