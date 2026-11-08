use serde::{
    Deserialize,
    Serialize,
};

use super::{
    PinnedItem,
    PipelineView,
};
use crate::domain::provider::ProviderSummary;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ViewSelectionQuery {
    pub pipeline_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewSelectionEntry {
    pub view: PipelineView,
    pub depth: usize,
    pub selected_count: usize,
    pub direct_count: usize,
    pub included_by_rule: bool,
    pub included_by_sub_view: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewMembershipChange {
    pub pipeline_ids: Vec<String>,
    pub member: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewEditorOptions {
    pub eligible_parents: Vec<PipelineView>,
    pub can_add_sub_view: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineNavigation {
    pub views: Vec<PipelineView>,
    pub pins: Vec<PinnedItem>,
    pub providers: Vec<ProviderSummary>,
    pub total_pipelines: usize,
}
