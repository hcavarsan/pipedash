mod aggregation;
mod batch;
mod config;
mod filters;
mod percentile;
mod processing;
mod query;
mod records;
mod retention;
mod rows;
mod stats;
#[cfg(test)]
mod tests;

use super::DatabasePool;
use crate::domain::MetricEntry;

pub struct RecordMetrics {
    pub run_number: i64,
    pub run_hash: String,
    pub entries: [Option<MetricEntry>; 3],
}

fn metadata_json(metric: &MetricEntry) -> String {
    metric
        .metadata
        .as_ref()
        .map_or_else(|| "null".to_string(), ToString::to_string)
}

pub struct MetricsRepository {
    pool: DatabasePool,
}

impl MetricsRepository {
    pub fn new_from_pool(pool: DatabasePool) -> Self {
        Self { pool }
    }
}
