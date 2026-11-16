use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::overrides::OverrideEntry;
use super::{
    ApplicationCapabilities,
    Confidence,
    IdentityKind,
    MemberSource,
    Stage,
};
use crate::domain::pipeline::{
    Pipeline,
    PipelineStatus,
};
use crate::domain::pipeline_view::SortDirection;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationSummary {
    pub id: i64,
    pub name: String,
    pub identity_kind: IdentityKind,
    pub capabilities: ApplicationCapabilities,
    pub chips: Vec<TargetChip>,
    pub arrows: Vec<ChipArrow>,
    pub build: Option<BuildSummary>,
    pub suggestion_count: usize,
    pub hidden: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChipKind {
    Target,
    Group,
    ShortLived,
    Summary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetChip {
    pub group_key: String,
    pub label: String,
    pub commit_short: Option<String>,
    pub status: PipelineStatus,
    pub health_label: Option<String>,
    pub behind_label: Option<String>,
    pub count: usize,
    pub kind: ChipKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ChipArrow {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildSummary {
    pub label: String,
    pub status: PipelineStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationList {
    pub applications: Vec<ApplicationSummary>,
    pub total: usize,
    pub suggestion_count: usize,
    pub config_error: Option<ConfigLoadError>,
    pub copy: ApplicationCopy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigLoadError {
    pub message: String,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationSort {
    #[default]
    Name,
    Health,
    Behind,
    LastDeploy,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ApplicationListQuery {
    pub search: Option<String>,
    pub health: Vec<PipelineStatus>,
    pub behind_only: bool,
    pub provider_ids: Vec<i64>,
    pub with_suggestions: bool,
    pub include_hidden: bool,
    pub sort: ApplicationSort,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationDetail {
    pub summary: ApplicationSummary,
    pub flow: Flow,
    pub members: Vec<ApplicationMember>,
    pub targets: Vec<ApplicationTarget>,
    pub suggestions: Vec<Suggestion>,
    pub overrides: Vec<OverrideEntry>,
    pub toml: Option<String>,
    pub config_defined: bool,
    pub copy: ApplicationCopy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationRef {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationTarget {
    pub target_key: String,
    pub name: String,
    pub group_key: String,
    pub group_label: String,
    pub short_lived: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flow {
    pub columns: u32,
    pub rows: u32,
    pub nodes: Vec<FlowNode>,
    pub links: Vec<FlowLink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlowNodeKind {
    Source,
    Build,
    Artifact,
    Target,
    Group,
    ShortLived,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowNode {
    pub id: String,
    pub kind: FlowNodeKind,
    pub column: u32,
    pub row: u32,
    pub title: String,
    pub subtitle: Option<String>,
    pub status: Option<PipelineStatus>,
    pub pipeline_id: Option<String>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowLink {
    pub id: String,
    pub from: String,
    pub to: String,
    pub confidence: Confidence,
    pub why: String,
    pub hint: Option<String>,
    pub edge_id: Option<i64>,
    pub suggestion_id: Option<String>,
    pub unlink_pipeline_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationMember {
    pub pipeline: Pipeline,
    pub stage: Stage,
    pub source: MemberSource,
    pub why: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionKind {
    SameApplication,
    Membership,
    BuiltFrom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    pub id: String,
    pub application_id: Option<i64>,
    pub kind: SuggestionKind,
    pub title: String,
    pub why: Vec<String>,
    pub rule_count: usize,
    pub bulk_accept: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangesPage {
    pub columns: Vec<ChangeColumn>,
    pub rows: Vec<ChangeRow>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeColumn {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeRow {
    pub commit_sha: String,
    pub commit_short: String,
    pub message: Option<String>,
    pub cells: Vec<ChangeCell>,
    pub segments: Vec<TimeSegment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CellState {
    Success,
    Failed,
    Running,
    NoEvidence,
    Live,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeCell {
    pub column: String,
    pub state: CellState,
    pub pipeline_id: Option<String>,
    pub run_number: Option<i64>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    Build,
    Wait,
    Deploy,
    BuildToDeployed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeSegment {
    pub kind: SegmentKind,
    pub seconds: i64,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationState {
    Live,
    Behind,
    NotDeployed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitLocation {
    pub application_id: i64,
    pub application_name: String,
    pub target_label: String,
    pub state: LocationState,
    pub label: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitLocations {
    pub title: String,
    pub entries: Vec<CommitLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployCommit {
    pub sha: String,
    pub short: String,
    pub message: Option<String>,
    pub authored_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeployChanges {
    pub title: String,
    pub commits: Vec<DeployCommit>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationCopy {
    pub title: String,
    pub search_placeholder: String,
    pub empty_title: String,
    pub empty_description: String,
    pub new_application: String,
    pub suggestions: String,
    pub suggestions_empty: String,
    pub health_filter: String,
    pub provider_filter: String,
    pub behind_only: String,
    pub with_suggestions: String,
    pub include_hidden: String,
    pub sort: String,
    pub sort_options: Vec<LabelOption>,
    pub health_options: Vec<LabelOption>,
    pub config_error_title: String,
    pub config_notice: String,
    pub tab_flow: String,
    pub tab_changes: String,
    pub tab_metrics: String,
    pub flow_empty: String,
    pub changes_empty: String,
    pub metrics_empty: String,
    pub legend_exact: String,
    pub legend_suggested: String,
    pub why_linked: String,
    pub edit: String,
    pub rename: String,
    pub hide: String,
    pub unhide: String,
    pub merge: String,
    pub split: String,
    pub members: String,
    pub targets: String,
    pub delete: String,
    pub copy_toml: String,
    pub copied: String,
    pub overrides: String,
    pub overrides_empty: String,
    pub revert: String,
    pub shadowed: String,
    pub accept: String,
    pub reject: String,
    pub accept_all: String,
    pub unlink: String,
    pub save: String,
    pub cancel: String,
    pub name: String,
    pub stage: String,
    pub stage_build: String,
    pub stage_deploy: String,
    pub add_member: String,
    pub remove: String,
    pub group: String,
    pub ungroup: String,
    pub direction: String,
    pub direction_from: String,
    pub direction_to: String,
    pub add_direction: String,
    pub all_targets: String,
    pub funnel: String,
    pub export: String,
    pub load_more: String,
    pub commit_column: String,
    pub warnings: String,
    pub back: String,
    pub hidden: String,
    pub sort_ascending: String,
    pub sort_descending: String,
}
