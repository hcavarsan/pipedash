use pipedash_plugin_api::PipelineStatus;

pub(crate) fn map_status(status: &str, conclusion: Option<&str>) -> PipelineStatus {
    match (status, conclusion) {
        ("completed", Some("success" | "neutral")) => PipelineStatus::Success,
        ("completed", Some("cancelled" | "stale")) => PipelineStatus::Cancelled,
        ("completed", Some("skipped")) => PipelineStatus::Skipped,
        ("completed", _) => PipelineStatus::Failed,
        ("in_progress", _) | ("queued", _) => PipelineStatus::Running,
        _ => PipelineStatus::Pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_documented_conclusion_maps_to_a_finished_status() {
        let cases = [
            ("success", PipelineStatus::Success),
            ("neutral", PipelineStatus::Success),
            ("failure", PipelineStatus::Failed),
            ("timed_out", PipelineStatus::Failed),
            ("startup_failure", PipelineStatus::Failed),
            ("action_required", PipelineStatus::Failed),
            ("cancelled", PipelineStatus::Cancelled),
            ("stale", PipelineStatus::Cancelled),
            ("skipped", PipelineStatus::Skipped),
        ];
        for (conclusion, expected) in cases {
            assert_eq!(
                map_status("completed", Some(conclusion)),
                expected,
                "conclusion {conclusion}"
            );
        }
    }

    #[test]
    fn unfinished_statuses_are_running_or_pending() {
        assert_eq!(map_status("in_progress", None), PipelineStatus::Running);
        assert_eq!(map_status("queued", None), PipelineStatus::Running);
        for status in ["requested", "waiting", "pending"] {
            assert_eq!(
                map_status(status, None),
                PipelineStatus::Pending,
                "status {status}"
            );
        }
    }
}
