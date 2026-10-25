pub mod defaults;
pub mod error;
pub mod health;
pub mod http;
pub mod plugin;
pub mod registry;
pub mod schema;
pub mod types;
pub mod utils;
pub mod workflow_dispatch;

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
    check_response,
    from_status,
    map_send_error,
    redact_url,
    retry_after,
    truncate_chars,
};
pub use plugin::{
    Plugin,
    PluginCapabilities,
    PluginMetadata,
};
pub use registry::PluginRegistry;
pub use schema::{
    ConfigField,
    ConfigFieldType,
    ConfigSchema,
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
    PipelineRun,
    PipelineStatus,
    RateLimit,
    ReviewDecision,
    TriggerParams,
    WorkflowParameter,
    WorkflowParameterType,
};
pub use utils::RetryPolicy;

#[macro_export]
macro_rules! register_plugin {
    ($plugin_type:ty) => {
        pub fn register(registry: &mut $crate::PluginRegistry) {
            registry.register(Box::new(<$plugin_type>::default()));
        }
    };
}
