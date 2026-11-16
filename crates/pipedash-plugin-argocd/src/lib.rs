mod backend;
mod client;
mod cluster;
mod config;
#[cfg(any(test, feature = "fixtures"))]
mod fixtures;
mod logs;
mod mapper;
mod metadata;
mod plugin;
mod schema;
mod sign_in;
mod types;

#[cfg(any(test, feature = "fixtures"))]
pub use fixtures::correlation_fixtures;
pub use plugin::ArgocdPlugin;

pipedash_plugin_api::register_plugin!(ArgocdPlugin);
