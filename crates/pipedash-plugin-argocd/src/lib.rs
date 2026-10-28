mod client;
mod config;
mod log_dialect;
mod logs;
mod mapper;
mod metadata;
mod plugin;
mod schema;
mod types;

pub use plugin::ArgocdPlugin;

pipedash_plugin_api::register_plugin!(ArgocdPlugin);
