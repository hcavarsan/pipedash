mod client;
mod config;
mod mapper;
mod plugin;
mod types;

pub use plugin::TektonPlugin;

pipedash_plugin_api::register_plugin!(TektonPlugin);
