pub mod error;
pub mod metrics;
pub mod pipeline;
pub mod provider;
pub mod validation;

pub use error::{
    DomainError,
    DomainResult,
};
pub use metrics::{
    AggregatedMetric,
    AggregatedMetrics,
    AggregationPeriod,
    AggregationType,
    GlobalMetricsConfig,
    MetricEntry,
    MetricMetadata,
    MetricType,
    MetricsConfig,
    MetricsConfigExport,
    MetricsQuery,
    MetricsStats,
    PipelineMetricsStats,
};
pub use pipeline::{
    PaginatedAvailablePipelines,
    PaginatedRunHistory,
    PaginationParams,
    Pipeline,
    PipelineRun,
    PipelineStatus,
    TriggerParams,
};
pub use provider::{
    FetchStatus,
    Provider,
    ProviderConfig,
    ProviderSummary,
};
pub use validation::{
    validate_config,
    validate_pagination,
    validate_pipeline_id,
    validate_provider_name,
    validate_provider_type,
    validate_trigger_params,
};
