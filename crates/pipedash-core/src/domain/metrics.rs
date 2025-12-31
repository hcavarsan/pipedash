use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    RunDuration,
    SuccessRate,
    RunFrequency,
}

impl MetricType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MetricType::RunDuration => "run_duration",
            MetricType::SuccessRate => "success_rate",
            MetricType::RunFrequency => "run_frequency",
        }
    }
}

impl std::str::FromStr for MetricType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "run_duration" => Ok(MetricType::RunDuration),
            "success_rate" => Ok(MetricType::SuccessRate),
            "run_frequency" => Ok(MetricType::RunFrequency),
            _ => Err(format!("Unknown metric type: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AggregationPeriod {
    Hourly,
    Daily,
    Weekly,
    Monthly,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AggregationType {
    Avg,
    Sum,
    Min,
    Max,
    P95,
    P99,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalMetricsConfig {
    pub enabled: bool,
    pub default_retention_days: i64,
    pub updated_at: DateTime<Utc>,
}

impl Default for GlobalMetricsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_retention_days: 7,
            updated_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub pipeline_id: String,
    pub enabled: bool,
    pub retention_days: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricsConfigExport {
    pub global_config: GlobalMetricsConfig,
    pub pipeline_configs: Vec<MetricsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricEntry {
    pub id: i64,
    pub pipeline_id: String,
    pub run_number: i64,
    pub timestamp: DateTime<Utc>,
    pub metric_type: MetricType,
    pub value: f64,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub run_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsQuery {
    pub pipeline_id: Option<String>,
    pub metric_type: Option<MetricType>,
    pub start_date: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub aggregation_period: Option<AggregationPeriod>,
    pub aggregation_type: Option<AggregationType>,
    pub limit: Option<usize>,
}

impl Default for MetricsQuery {
    fn default() -> Self {
        Self {
            pipeline_id: None,
            metric_type: None,
            start_date: None,
            end_date: None,
            aggregation_period: None,
            aggregation_type: None,
            limit: Some(1000),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedMetric {
    pub timestamp: DateTime<Utc>,
    pub value: f64,
    pub count: i64,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub avg: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedMetrics {
    pub metrics: Vec<AggregatedMetric>,
    pub total_count: usize,
    pub metric_type: MetricType,
    pub aggregation_period: AggregationPeriod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsStats {
    pub total_metrics_count: i64,
    pub estimated_size_bytes: i64,
    pub estimated_size_mb: f64,
    pub last_cleanup_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
    pub by_pipeline: Vec<PipelineMetricsStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineMetricsStats {
    pub pipeline_id: String,
    pub pipeline_name: String,
    pub metrics_count: i64,
    pub oldest_metric: Option<DateTime<Utc>>,
    pub newest_metric: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricMetadata {
    pub status: Option<String>,
    pub branch: Option<String>,
    pub repository: Option<String>,
    pub actor: Option<String>,
}

impl MetricMetadata {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}
