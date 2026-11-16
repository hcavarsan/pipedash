mod metadata;
mod plugin;
mod schema;

pub use plugin::OciRegistryPlugin;

#[cfg(any(test, feature = "fixtures"))]
pub fn correlation_fixtures() -> Vec<(
    pipedash_plugin_api::Pipeline,
    Vec<pipedash_plugin_api::PipelineRun>,
)> {
    Vec::new()
}

pipedash_plugin_api::register_plugin!(OciRegistryPlugin);
