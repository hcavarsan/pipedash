pub mod defaults;
pub mod error;
pub mod health;
pub mod http;
pub mod logs;
pub mod plugin;
pub mod registry;
pub mod schema;
pub mod sign_in;
pub mod types;
pub mod utils;

pub use error::{
    PluginError,
    PluginResult,
};
pub use health::{
    collect_controller_health,
    custom_status_source,
    status_page_field,
    ClientOptions,
    ComponentFilter,
    ComponentHealth,
    FeedFormat,
    HealthLevel,
    StatusSource,
    CONTROLLER_HEALTH_FEATURE,
    STATUS_PAGE_URL_KEY,
};
pub use http::{
    body_preview,
    check_response,
    from_status,
    map_send_error,
    read_prefix,
    redact_url,
    retry_after,
    truncate_chars,
};
pub use logs::{
    content_hash,
    extend_content_hash,
    next_log_chunk,
    ContentRange,
    LineMarker,
    LogAvailability,
    LogCursor,
    LogDialect,
    LogNode,
    LogNodeKind,
    LogRead,
    LogReadMode,
    LogSink,
    ParsedLine,
    PlainDialect,
    RangedRead,
    RunRef,
    CONTENT_HASH_SEED,
    LOG_CHUNK_STALL_TIMEOUT,
};
pub use plugin::{
    Plugin,
    PluginCapabilities,
    PluginMetadata,
};
pub use registry::PluginRegistry;
pub use schema::{
    is_no_auth,
    AuthMode,
    ConfigField,
    ConfigFieldType,
    ConfigSchema,
    ConfigSection,
    AUTH_MODE_KEY,
};
pub use sign_in::{
    SignInClient,
    SignInToken,
};
pub use types::{
    AvailablePipeline,
    BuildAgent,
    BuildArtifact,
    BuildQueue,
    Feature,
    FeatureAvailability,
    Organization,
    PaginatedAvailablePipelines,
    PaginatedResponse,
    PaginationParams,
    Permission,
    PermissionCheck,
    PermissionStatus,
    Pipeline,
    PipelineCatalog,
    PipelineRun,
    PipelineStatus,
    RateLimit,
    ResourceState,
    ReviewDecision,
    TriggerParams,
    WorkflowParameter,
    WorkflowParameterType,
    READ_ONLY_METADATA_KEY,
};
pub use utils::{
    retain_with_ci,
    RetryPolicy,
};

#[macro_export]
macro_rules! register_plugin {
    ($plugin_type:ty) => {
        pub fn register(registry: &mut $crate::PluginRegistry) {
            registry.register(Box::new(<$plugin_type>::default()));
        }
    };
}
