mod client;
mod config;
#[cfg(any(test, feature = "fixtures"))]
mod fixtures;
mod log_dialect;
mod mapper;
mod metadata;
mod plugin;
mod schema;
mod stage_cursor;
mod types;

#[cfg(any(test, feature = "fixtures"))]
pub use fixtures::correlation_fixtures;
pub use plugin::KomodoPlugin;

pipedash_plugin_api::register_plugin!(KomodoPlugin);
