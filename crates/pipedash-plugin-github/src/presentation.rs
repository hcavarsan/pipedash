use pipedash_plugin_api::PluginPresentation;

pub(crate) fn presentation() -> PluginPresentation {
    PluginPresentation {
        force_cancel_confirmation: "Force cancel only after normal cancellation fails. GitHub bypasses conditions such as always(). You can't undo this.".to_string(),
        ..PluginPresentation::new("GitHub Actions")
    }
}
