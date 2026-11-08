mod client;
mod config;
mod logs;
mod mapper;
mod metadata;
mod plugin;
mod schema;
mod sign_in;
mod types;

pub use plugin::ArgocdPlugin;

pipedash_plugin_api::register_plugin!(ArgocdPlugin);
