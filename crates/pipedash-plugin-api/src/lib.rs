pub mod actions;
pub mod build_watch;
pub mod builders;
pub mod columns;
pub mod config;
pub mod defaults;
pub mod delegate;
pub mod error;
pub mod health;
pub mod history;
pub mod http;
#[cfg(feature = "kubernetes")]
pub mod kubernetes;
pub mod logs;
pub mod metadata;
pub mod pagination;
pub mod plugin;
pub mod presentation;
pub mod registry;
pub mod schema;
pub mod sign_in;
pub mod support;
#[cfg(test)]
mod testing;
pub mod types;
pub mod utils;

pub use actions::{
    ActionAvailability,
    ActionIcon,
    PipelineActionState,
    RerunStrategy,
    RunActionState,
    TriggerResult,
};
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
pub use history::{
    RunHistoryOrder,
    RunHistoryPolicy,
};
pub use http::{
    body_preview,
    check_response,
    from_status,
    map_send_error,
    read_prefix,
    redact_url,
    require_client,
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
    RunRef,
    CONTENT_HASH_SEED,
    LOG_CHUNK_STALL_TIMEOUT,
};
pub use metadata::{
    PluginCapabilities,
    PluginManifest,
    PluginMetadata,
};
pub use plugin::Plugin;
pub use presentation::{
    ActionCopy,
    DisplayField,
    Notice,
    PipelineFacts,
    PipelineTerminology,
    PluginPresentation,
    RunDisplay,
    RunFacts,
    StatusOption,
    ViewCopy,
    WizardCopy,
};
pub use registry::PluginRegistry;
pub use schema::{
    is_no_auth,
    AuthMode,
    AuthOption,
    ConfigField,
    ConfigSchema,
    ConfigSection,
    AUTH_MODE_KEY,
};
pub use sign_in::{
    SignInClient,
    SignInRefreshRejection,
    SignInToken,
};
pub use types::{
    AvailablePipeline,
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
    selected_items,
    RetryPolicy,
};

#[macro_export]
macro_rules! register_plugin {
    ($plugin_type:ty) => {
        pub fn register(registry: &mut $crate::PluginRegistry) {
            registry.register(
                || Box::new(<$plugin_type>::default()),
                $crate::PluginManifest {
                    package_name: env!("CARGO_PKG_NAME"),
                    version: env!("CARGO_PKG_VERSION"),
                    description: env!("CARGO_PKG_DESCRIPTION"),
                    authors: env!("CARGO_PKG_AUTHORS"),
                    icon_svg: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icon.svg")),
                },
            );
        }
    };
}
