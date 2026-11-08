use std::collections::HashMap;

use serde::{
    Deserialize,
    Serialize,
};

use crate::types::{
    PipelineStatus,
    ResourceState,
};

mod defaults;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayField {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusOption {
    pub value: PipelineStatus,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipelineTerminology {
    pub item: String,
    pub items: String,
    pub group: String,
    pub groups: String,
    pub group_title: String,
    pub source_title: String,
    pub sources_title: String,
    pub history: String,
    pub trigger: String,
    pub run: String,
    pub runs: String,
    pub organization_label: String,
    pub section_count_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WizardCopy {
    pub credentials_description: String,
    pub name_placeholder: String,
    pub permissions_title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionCopy {
    pub label: String,
    pub compact_label: String,
    pub title: String,
    pub success: Notice,
    pub error_title: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewCopy {
    pub title: String,
    pub list_title: String,
    pub search_placeholder: String,
    pub matching_empty: Notice,
    pub hidden_empty: Notice,
    pub empty: Notice,
    pub pinned_empty: Notice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginPresentation {
    pub terminology: PipelineTerminology,
    pub status_options: Vec<StatusOption>,
    pub wizard: WizardCopy,
    pub view: ViewCopy,
    pub trigger: ActionCopy,
    pub rerun: ActionCopy,
    pub history_note: Option<String>,
    pub empty_history_description: String,
    pub run_details_title: String,
    pub progress_title: String,
    pub archived_notice: Option<Notice>,
    pub force_cancel_confirmation: String,
}

impl PluginPresentation {
    pub fn new(provider_name: &str) -> Self {
        defaults::presentation(provider_name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RunDisplay {
    pub title: String,
    pub message: Option<String>,
    pub context: Vec<DisplayField>,
    pub started_label: String,
}

#[derive(Debug, Clone, Copy)]
pub struct PipelineFacts<'a> {
    pub id: &'a str,
    pub status: PipelineStatus,
    pub branch: Option<&'a str>,
    pub workflow_file: Option<&'a str>,
    pub resource_state: Option<&'a ResourceState>,
    pub metadata: &'a HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy)]
pub struct RunFacts<'a> {
    pub run_number: i64,
    pub status: PipelineStatus,
    pub branch: Option<&'a str>,
    pub actor: Option<&'a str>,
    pub commit_message: Option<&'a str>,
    pub inputs: Option<&'a serde_json::Value>,
    pub metadata: &'a HashMap<String, serde_json::Value>,
}
