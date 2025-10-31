//! Data mapping utilities for GitHub Actions

use pipedash_plugin_api::PipelineStatus;

/// Maps GitHub Actions workflow status and conclusion to PipelineStatus
pub(crate) fn map_status(status: &str, conclusion: Option<&str>) -> PipelineStatus {
    match (status, conclusion) {
        ("completed", Some("success")) => PipelineStatus::Success,
        ("completed", Some("failure")) => PipelineStatus::Failed,
        ("completed", Some("cancelled")) => PipelineStatus::Cancelled,
        ("completed", Some("skipped")) => PipelineStatus::Skipped,
        ("in_progress", _) | ("queued", _) => PipelineStatus::Running,
        _ => PipelineStatus::Pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map_status() {
        assert_eq!(
            map_status("completed", Some("success")),
            PipelineStatus::Success
        );
        assert_eq!(
            map_status("completed", Some("failure")),
            PipelineStatus::Failed
        );
        assert_eq!(
            map_status("completed", Some("cancelled")),
            PipelineStatus::Cancelled
        );
        assert_eq!(map_status("in_progress", None), PipelineStatus::Running);
        assert_eq!(map_status("queued", None), PipelineStatus::Running);
    }
}
