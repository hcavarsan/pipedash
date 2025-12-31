pub mod defaults;
pub mod error;
pub mod plugin;
pub mod registry;
pub mod schema;
pub mod types;
pub mod utils;

pub use error::{
    PluginError,
    PluginResult,
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
