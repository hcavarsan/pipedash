use pipedash_plugin_api::PluginPresentation;

pub(crate) fn presentation() -> PluginPresentation {
    PluginPresentation {
        force_cancel_confirmation: "Force cancel only after normal cancellation fails. GitHub bypasses conditions such as always(). You can't undo this.".to_string(),
        rerun_job_confirmation: "Runs this job again. GitHub also reruns the jobs that depend on it.".to_string(),
        ..PluginPresentation::new("GitHub Actions")
    }
}
