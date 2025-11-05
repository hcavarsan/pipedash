mod client;
mod config;
mod mapper;
mod metadata;
mod plugin;
mod schema;
mod types;

pub use plugin::TektonPlugin;

pipedash_plugin_api::register_plugin!(TektonPlugin);
