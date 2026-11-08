pub(crate) fn plugin_status(
    status: &crate::domain::PipelineStatus,
) -> pipedash_plugin_api::PipelineStatus {
    use pipedash_plugin_api::PipelineStatus as Api;

    use crate::domain::PipelineStatus as Core;
    match status {
        Core::Success => Api::Success,
        Core::Failed => Api::Failed,
        Core::Degraded => Api::Degraded,
        Core::Running => Api::Running,
        Core::Pending => Api::Pending,
        Core::Cancelled => Api::Cancelled,
        Core::Skipped => Api::Skipped,
    }
}
