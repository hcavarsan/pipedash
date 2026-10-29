mod client;
mod config;
mod log_dialect;
mod log_tail;
mod mapper;
mod metadata;
mod plugin;
mod schema;
mod types;

pub use log_dialect::AzureDevOpsLogDialect;
pub use plugin::AzureDevOpsPlugin;

pipedash_plugin_api::register_plugin!(AzureDevOpsPlugin);
