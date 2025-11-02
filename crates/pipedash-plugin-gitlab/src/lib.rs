mod client;
mod config;
mod mapper;
mod plugin;
mod types;

pub use plugin::GitLabPlugin;

pipedash_plugin_api::register_plugin!(GitLabPlugin);
