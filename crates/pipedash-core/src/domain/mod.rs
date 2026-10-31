pub mod credential;
pub mod error;
pub mod health;
pub mod metrics;
pub mod pipeline;
pub mod pipeline_view;
pub mod provider;
pub mod push;
pub mod refresh;
pub mod run_logs;
pub mod validation;

pub use credential::{
    SignInCredential,
    SignInRef,
    SignInStart,
    SignInStatus,
    SIGN_IN_CONFIG_KEY,
    SIGN_IN_EXPIRED,
};
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
    ResourceState,
    TriggerParams,
};
pub use pipeline_view::{
    CreateViewInput,
    DateRange,
    LegacyImportResult,
    LegacyMarks,
    MembershipSource,
    PinKind,
    PinnedBlock,
    PinnedItem,
    PipelineMenuEntry,
    PipelineMenuSnapshot,
    PipelineView,
    PipelineViewPage,
    PipelineViewQuery,
    RuleMatcher,
    SectionKind,
    SectionMode,
    SortDirection,
    SortKey,
    ViewCapabilities,
    ViewDisplay,
    ViewFacets,
    ViewKind,
    ViewLayout,
    ViewMember,
    ViewMembership,
    ViewPatch,
    ViewPipeline,
    ViewRef,
    ViewRules,
    ViewSection,
};
pub use provider::{
    FetchStatus,
    FreshnessState,
    Provider,
    ProviderConfig,
    ProviderFreshness,
    ProviderImportResult,
    ProviderRecovery,
    ProviderSummary,
};
pub use push::PushSubscription;
pub use refresh::{
    RefreshResult,
    RefreshState,
};
pub use run_logs::{
    AnsiColor,
    LineKind,
    LineMarker,
    LogAvailability,
    LogCacheStats,
    LogCursor,
    LogDialect,
    LogDocumentMeta,
    LogLine,
    LogLinesPage,
    LogNode,
    LogNodeKind,
    LogRead,
    LogReadMode,
    LogSearchQuery,
    LogSearchResult,
    LogSection,
    LogSectionKind,
    LogSink,
    LogStyle,
    ParsedLine,
    PlainDialect,
    RunLogDocument,
    RunLogNode,
    RunLogTree,
};
pub use validation::{
    validate_config,
    validate_pagination,
    validate_pipeline_id,
    validate_provider_name,
    validate_provider_type,
    validate_trigger_params,
};
