mod actions;
mod client;
mod config;
mod health;
mod history;
mod logs;
mod mapper;
mod metadata;
mod plugin;
mod presentation;
mod schema;
mod types;
mod workloads;

pub use plugin::FluxcdPlugin;

pipedash_plugin_api::register_plugin!(FluxcdPlugin);
