mod client;
mod config;
mod log_cursor;
mod log_dialect;
mod mapper;
mod metadata;
mod plugin;
mod schema;
mod types;

pub use plugin::CoolifyPlugin;

pipedash_plugin_api::register_plugin!(CoolifyPlugin);
