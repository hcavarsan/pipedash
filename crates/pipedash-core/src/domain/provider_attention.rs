use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::error::FailureKind;
use super::health::{
    HealthSignal,
    Maintenance,
};
use super::provider::ProviderRecovery;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionTone {
    Warning,
    Danger,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderOpenAction {
    #[default]
    Pipelines,
    Status,
    SignIn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AttentionSchedule {
    CheckingNow,
    NextAttempt { at: DateTime<Utc> },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAttention {
    pub tone: Option<AttentionTone>,
    pub headline: Option<String>,
    pub detail: Option<String>,
    pub hint: Option<String>,
    pub fetch_hint: Option<String>,
    pub schedule: Option<AttentionSchedule>,
    pub kind: Option<FailureKind>,
    pub recovery: Option<ProviderRecovery>,
    pub episode_key: Option<String>,
    pub refreshing_normally: bool,
    pub stale_signals: Vec<HealthSignal>,
    pub scheduled_maintenances: Vec<Maintenance>,
    pub open_action: ProviderOpenAction,
}
