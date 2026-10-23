pub mod error;
pub mod health;
pub mod metrics;
pub mod pipeline;
pub mod provider;
pub mod refresh;
pub mod validation;

pub use error::{
    DomainError,
    DomainResult,
    FailureKind,
};
pub use health::{
    ComponentHealth,
    EvidenceView,
    HealthLevel,
    HealthSignal,
    Incident,
    Maintenance,
    ProviderHealth,
    SignalKind,
    StatusPageRef,
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
pub use refresh::{
    RefreshResult,
    RefreshState,
};
pub use validation::{
    validate_config,
    validate_pagination,
    validate_pipeline_id,
    validate_provider_name,
    validate_provider_type,
    validate_trigger_params,
};
