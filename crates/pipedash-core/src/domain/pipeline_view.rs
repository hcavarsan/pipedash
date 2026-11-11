use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Deserializer,
    Serialize,
};

pub mod presentation;
pub mod reference;
pub mod rules;
pub mod selection;

use presentation::{
    SectionPresentation,
    ViewPresentation,
};
use reference::{
    ViewKind,
    ViewRef,
};
use rules::ViewRules;

use super::date_range::DateRange;
use super::pipeline::{
    Pipeline,
    PipelineStatus,
};
use super::provider::ProviderFreshness;

pub(super) fn set_or_clear<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

pub(super) fn status_list<'de, D>(deserializer: D) -> Result<Vec<PipelineStatus>, D::Error>
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
    #[serde(default)]
    pub presentation: SectionPresentation,
    #[serde(default)]
    pub freshness: ProviderFreshness,
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
pub struct PipelineMenuSnapshot {
    pub pipelines: Vec<PipelineMenuEntry>,
    pub shortcuts: Vec<PinnedItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineMenuEntry {
    pub pipeline_id: String,
    pub pipeline: Option<Pipeline>,
    pub provider_name: Option<String>,
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
    pub section_count: usize,
    pub facets: ViewFacets,
    #[serde(default)]
    pub presentation: ViewPresentation,
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
mod tests;
