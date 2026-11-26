mod config;
mod deck;
#[cfg(any(test, feature = "fixtures"))]
mod fixtures;
mod mapper;
mod metadata;
mod plugin;
mod prowjobs;
mod schema;
mod sign_in;
mod storage;
#[cfg(test)]
mod test_support;
mod types;

#[cfg(any(test, feature = "fixtures"))]
pub use fixtures::correlation_fixtures;
pub use plugin::ProwPlugin;

pipedash_plugin_api::register_plugin!(ProwPlugin);
