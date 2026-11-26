mod client;
mod config;
#[cfg(any(test, feature = "fixtures"))]
mod fixtures;
mod log_dialect;
mod mapper;
mod metadata;
mod plugin;
mod schema;
#[cfg(test)]
mod test_support;
mod types;

#[cfg(any(test, feature = "fixtures"))]
pub use fixtures::correlation_fixtures;
pub use log_dialect::BuildkiteLogDialect;
pub use plugin::BuildkitePlugin;

pipedash_plugin_api::register_plugin!(BuildkitePlugin);
