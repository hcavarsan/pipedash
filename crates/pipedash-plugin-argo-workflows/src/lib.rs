mod client;
mod config;
mod fixtures;
mod mapper;
mod metadata;
mod plugin;
mod schema;
mod types;

pub use fixtures::correlation_fixtures;
pub use plugin::ArgoWorkflowsPlugin;

pipedash_plugin_api::register_plugin!(ArgoWorkflowsPlugin);
