// Core plugin API for Pipedash
//
// This crate defines the plugin interface that all provider plugins must
// implement. It includes common types, traits, and the plugin registry system.

pub mod error;
pub mod plugin;
pub mod registry;
pub mod schema;
pub mod types;
pub mod utils;

// Re-export main types for convenience
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
    Pipeline,
    PipelineRun,
    PipelineStatus,
    TriggerParams,
    WorkflowParameter,
    WorkflowParameterType,
};
pub use utils::RetryPolicy;

/// Macro for registering plugins
///
/// Usage:
/// ```ignore
/// use pipedash_plugin_api::register_plugin;
///
/// register_plugin!(MyPlugin);
/// ```
#[macro_export]
macro_rules! register_plugin {
    ($plugin_type:ty) => {
        pub fn register(registry: &mut $crate::PluginRegistry) {
            registry.register(Box::new(<$plugin_type>::default()));
        }
    };
}
