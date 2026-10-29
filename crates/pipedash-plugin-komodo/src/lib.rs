mod client;
mod config;
mod log_dialect;
mod mapper;
mod metadata;
mod plugin;
mod schema;
mod types;

pub use log_dialect::KomodoLogDialect;
pub use plugin::KomodoPlugin;

pipedash_plugin_api::register_plugin!(KomodoPlugin);
