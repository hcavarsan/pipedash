use serde::{
    Deserialize,
    Serialize,
};

use super::{
    AggregatedMetrics,
    AggregationPeriod,
    AggregationType,
    DateRange,
    MetricType,
    SortDirection,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricSortField {
    Timestamp,
    Value,
    Count,
    Min,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricTableSort {
    pub field: MetricSortField,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsDashboardQuery {
    #[serde(default)]
    pub pipeline_id: Option<String>,
    pub metric_type: MetricType,
    pub aggregation_period: AggregationPeriod,
    #[serde(default)]
    pub aggregation_type: Option<AggregationType>,
    pub date_range: DateRange,
    #[serde(default)]
    pub utc_offset_minutes: i32,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub table_sort: Option<MetricTableSort>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricStats {
    pub average: Option<f64>,
    pub latest: Option<f64>,
    pub max: Option<f64>,
    pub observed: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricRangeOption {
    pub value: DateRange,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsDashboard {
    pub data: AggregatedMetrics,
    pub observed: Vec<bool>,
    pub stats: MetricStats,
    pub table_indices: Vec<usize>,
    pub ranges: Vec<MetricRangeOption>,
    pub widest_range: DateRange,
    pub aggregation_type: AggregationType,
}
