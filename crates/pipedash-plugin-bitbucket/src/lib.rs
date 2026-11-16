mod client;
mod config;
#[cfg(any(test, feature = "fixtures"))]
mod fixtures;
mod log_dialect;
mod mapper;
mod metadata;
mod plugin;
mod schema;
mod types;

#[cfg(any(test, feature = "fixtures"))]
pub use fixtures::correlation_fixtures;
pub use log_dialect::BitbucketLogDialect;
pub use plugin::BitbucketPlugin;

pipedash_plugin_api::register_plugin!(BitbucketPlugin);
