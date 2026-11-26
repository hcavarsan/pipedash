mod client;
mod config;
#[cfg(any(test, feature = "fixtures"))]
mod fixtures;
mod log_dialect;
mod mapper;
mod metadata;
mod plugin;
mod schema;
mod step_log;
mod types;

#[cfg(any(test, feature = "fixtures"))]
pub use fixtures::correlation_fixtures;
pub use log_dialect::TektonLogDialect;
pub use plugin::TektonPlugin;

pipedash_plugin_api::register_plugin!(TektonPlugin);
