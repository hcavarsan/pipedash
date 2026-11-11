mod client;
mod config;
#[cfg(any(test, feature = "fixtures"))]
mod fixtures;
mod log_dialect;
mod log_tail;
mod mapper;
mod metadata;
mod plugin;
mod schema;
mod types;

#[cfg(any(test, feature = "fixtures"))]
pub use fixtures::correlation_fixtures;
pub use log_dialect::AzureDevOpsLogDialect;
pub use plugin::AzureDevOpsPlugin;

pipedash_plugin_api::register_plugin!(AzureDevOpsPlugin);
