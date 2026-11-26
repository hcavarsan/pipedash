mod client;
mod config;
mod correlation;
#[cfg(any(test, feature = "fixtures"))]
mod fixtures;
mod mapper;
mod metadata;
mod permissions;
mod plugin;
mod presentation;
mod schema;
mod types;

#[cfg(any(test, feature = "fixtures"))]
pub use fixtures::correlation_fixtures;
pub use plugin::GitHubPlugin;

pipedash_plugin_api::register_plugin!(GitHubPlugin);
