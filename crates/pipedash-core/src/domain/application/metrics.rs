use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::dto::ChangeColumn;
use crate::domain::metrics::AggregationPeriod;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationMetricsQuery {
    pub application_id: i64,
    #[serde(default)]
    pub group_key: Option<String>,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub period: AggregationPeriod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationMetrics {
    pub metrics: Vec<MetricResult>,
    pub funnel: Vec<FunnelStep>,
    pub groups: Vec<ChangeColumn>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricResult {
    pub id: MetricId,
    pub label: String,
    pub unit: MetricUnit,
    pub value: Option<f64>,
    pub p50: Option<f64>,
    pub p95: Option<f64>,
    pub unavailable_reason: Option<String>,
    pub coverage: Coverage,
    pub series: Vec<MetricPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage {
    pub counted: usize,
    pub excluded: usize,
    pub since: Option<DateTime<Utc>>,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricPoint {
    pub timestamp: DateTime<Utc>,
    pub value: f64,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunnelStep {
    pub key: String,
    pub label: String,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricId {
    LeadTime,
    DeliveryTime,
    BuildTime,
    WaitTime,
    BuildToDeployed,
    DeployTime,
    DeployFrequency,
    BuildSuccessRate,
    DeployFailures,
    ChangeFailureRate,
    RecoveryTime,
    Drift,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricUnit {
    Seconds,
    Percent,
    Count,
    Commits,
}
