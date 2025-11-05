mod client;
mod config;
mod mapper;
mod metadata;
mod plugin;
mod schema;
mod types;

pub use plugin::GitLabPlugin;

pipedash_plugin_api::register_plugin!(GitLabPlugin);
