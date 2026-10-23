mod client;
mod config;
mod mapper;
mod metadata;
mod plugin;
mod schema;
#[cfg(test)]
mod test_support;
mod types;

pub use plugin::JenkinsPlugin;

pipedash_plugin_api::register_plugin!(JenkinsPlugin);
