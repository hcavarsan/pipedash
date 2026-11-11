use serde::{
    Deserialize,
    Serialize,
};

use crate::presentation::DisplayField;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ActionAvailability {
    pub allowed: bool,
    pub reason: Option<String>,
}

impl ActionAvailability {
    pub fn allowed(allowed: bool) -> Self {
        Self {
            allowed,
            reason: None,
        }
    }

    pub fn blocked(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RerunStrategy {
    InPlace,
    #[default]
    ReplayInputs,
    ReconcileCurrent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActionIcon {
    #[default]
    Play,
    Refresh,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PipelineActionState {
    pub trigger: ActionAvailability,
    pub cancel: ActionAvailability,
    pub force_cancel: ActionAvailability,
    pub enable: ActionAvailability,
    pub rerun_strategy: RerunStrategy,
    pub trigger_icon: ActionIcon,
    pub target: Option<DisplayField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RunActionState {
    pub cancel: ActionAvailability,
    pub force_cancel: ActionAvailability,
    pub rerun: ActionAvailability,
    pub rerun_failed: ActionAvailability,
    pub review: ActionAvailability,
    pub watch: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TriggerResult {
    pub run_id: Option<String>,
    pub run_number: Option<i64>,
}
