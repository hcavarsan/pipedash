use chrono::{
    DateTime,
    Utc,
};
pub use pipedash_plugin_api::{
    ComponentHealth,
    HealthLevel,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalKind {
    StatusPage,
    Instance,
    Connection,
    RateLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceView {
    Fresh,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incident {
    pub id: String,
    pub name: String,
    pub level: HealthLevel,
    pub phase: String,
    pub latest_update: Option<String>,
    pub url: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Maintenance {
    pub id: String,
    pub name: String,
    pub in_progress: bool,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthSignal {
    pub kind: SignalKind,
    pub episode_id: String,
    pub level: HealthLevel,
    pub evidence: EvidenceView,
    pub observed_at: DateTime<Utc>,
    pub title: String,
    pub detail: Option<String>,
    pub reported_by: String,
    pub url: Option<String>,
    pub components: Vec<ComponentHealth>,
    pub incidents: Vec<Incident>,
    pub maintenances: Vec<Maintenance>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusPageRef {
    pub name: String,
    pub url: String,
    pub checked_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub level: HealthLevel,
    pub signals: Vec<HealthSignal>,
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub in_flight: bool,
    pub status_page: Option<StatusPageRef>,
}

impl Default for ProviderHealth {
    fn default() -> Self {
        Self {
            level: HealthLevel::Operational,
            signals: Vec::new(),
            next_attempt_at: None,
            in_flight: false,
            status_page: None,
        }
    }
}
