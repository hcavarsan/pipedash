mod client;
mod config;
mod log_dialect;
mod mapper;
mod metadata;
mod plugin;
mod schema;
mod types;

pub use log_dialect::WoodpeckerLogDialect;
pub use plugin::WoodpeckerPlugin;

pipedash_plugin_api::register_plugin!(WoodpeckerPlugin);
