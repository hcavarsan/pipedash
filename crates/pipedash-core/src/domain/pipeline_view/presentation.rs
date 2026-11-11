use pipedash_plugin_api::{
    PipelineTerminology,
    PluginPresentation,
    StatusOption,
    ViewCopy,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewPresentation {
    pub terminology: PipelineTerminology,
    pub status_options: Vec<StatusOption>,
    pub copy: ViewCopy,
}

impl Default for ViewPresentation {
    fn default() -> Self {
        let defaults = PluginPresentation::new("");
        Self {
            terminology: PipelineTerminology::default(),
            status_options: defaults.status_options,
            copy: ViewCopy::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionPresentation {
    pub organization_label: String,
    pub count_label: String,
}

impl Default for SectionPresentation {
    fn default() -> Self {
        let terminology = PipelineTerminology::default();
        Self {
            organization_label: terminology.organization_label,
            count_label: terminology.section_count_label,
        }
    }
}
