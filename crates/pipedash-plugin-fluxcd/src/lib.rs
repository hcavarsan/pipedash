mod client;
mod config;
mod health;
mod log_dialect;
mod logs;
mod mapper;
mod metadata;
mod plugin;
mod schema;
mod types;
mod workloads;

pub use plugin::FluxcdPlugin;

pipedash_plugin_api::register_plugin!(FluxcdPlugin);
