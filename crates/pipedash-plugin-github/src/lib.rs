mod client;
mod config;
mod mapper;
mod metadata;
mod permissions;
mod plugin;
mod schema;
mod types;

pub use plugin::GitHubPlugin;

pipedash_plugin_api::register_plugin!(GitHubPlugin);
