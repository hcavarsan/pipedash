mod client;
mod config;
mod log_dialect;
mod mapper;
mod metadata;
mod parameters;
mod plugin;
mod schema;
mod types;

pub use plugin::CircleCIPlugin;

pipedash_plugin_api::register_plugin!(CircleCIPlugin);
