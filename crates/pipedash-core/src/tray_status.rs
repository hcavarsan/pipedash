//! Tray status aggregation for the macOS menu bar feature.
//!
//! This module provides functionality to aggregate the status of pinned pipelines
//! and return a summary suitable for display in the system tray.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{Pipeline, PipelineStatus};

/// Aggregate status for the tray icon.
/// Priority order: Running > Failed > Cancelled > Success > Unknown
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrayStatus {
    /// At least one pipeline is currently running
    Running,
    /// At least one pipeline has failed (and none are running)
    Failed,
    /// At least one pipeline was cancelled (and none are running or failed)
    Cancelled,
    /// All pipelines have passed
    Passed,
    /// No pinned pipelines or unknown state
    Unknown,
}

impl TrayStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrayStatus::Running => "running",
            TrayStatus::Failed => "failed",
            TrayStatus::Cancelled => "cancelled",
            TrayStatus::Passed => "passed",
            TrayStatus::Unknown => "unknown",
        }
    }

    /// Get the icon filename for this status
    pub fn icon_name(&self) -> &'static str {
        match self {
            TrayStatus::Running => "tray-running.png",
            TrayStatus::Failed => "tray-failed.png",
            TrayStatus::Cancelled => "tray-cancelled.png",
            TrayStatus::Passed => "tray-passed.png",
            TrayStatus::Unknown => "tray-idle.png",
        }
    }
}

/// Summary of a pinned pipeline for display in the tray menu
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedPipelineSummary {
    pub id: String,
    pub name: String,
    pub status: PipelineStatus,
    pub last_run: Option<DateTime<Utc>>,
    pub repository: String,
    pub provider_type: String,
}

impl From<&Pipeline> for PinnedPipelineSummary {
    fn from(pipeline: &Pipeline) -> Self {
        Self {
            id: pipeline.id.clone(),
            name: pipeline.name.clone(),
            status: pipeline.status.clone(),
            last_run: pipeline.last_run,
            repository: pipeline.repository.clone(),
            provider_type: pipeline.provider_type.clone(),
        }
    }
}

/// Aggregated tray status with list of pinned pipelines
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrayStatusSummary {
    pub overall_status: TrayStatus,
    pub pinned_pipelines: Vec<PinnedPipelineSummary>,
    pub total_count: usize,
    pub running_count: usize,
    pub failed_count: usize,
    pub passed_count: usize,
}

impl TrayStatusSummary {
    /// Create a summary from a list of pinned pipelines
    pub fn from_pipelines(pipelines: &[Pipeline]) -> Self {
        let mut running_count = 0;
        let mut failed_count = 0;
        let mut cancelled_count = 0;
        let mut passed_count = 0;

        for pipeline in pipelines {
            match pipeline.status {
                PipelineStatus::Running | PipelineStatus::Pending => running_count += 1,
                PipelineStatus::Failed => failed_count += 1,
                PipelineStatus::Cancelled => cancelled_count += 1,
                PipelineStatus::Success => passed_count += 1,
                PipelineStatus::Skipped => passed_count += 1,
            }
        }

        let overall_status = if pipelines.is_empty() {
            TrayStatus::Unknown
        } else if running_count > 0 {
            TrayStatus::Running
        } else if failed_count > 0 {
            TrayStatus::Failed
        } else if cancelled_count > 0 {
            TrayStatus::Cancelled
        } else if passed_count > 0 {
            TrayStatus::Passed
        } else {
            TrayStatus::Unknown
        };

        let pinned_pipelines: Vec<PinnedPipelineSummary> =
            pipelines.iter().map(PinnedPipelineSummary::from).collect();

        Self {
            overall_status,
            pinned_pipelines,
            total_count: pipelines.len(),
            running_count,
            failed_count,
            passed_count,
        }
    }

    /// Check if there are any pinned pipelines
    pub fn has_pinned_pipelines(&self) -> bool {
        !self.pinned_pipelines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;

    fn make_pipeline(id: &str, name: &str, status: PipelineStatus) -> Pipeline {
        Pipeline {
            id: id.to_string(),
            provider_id: 1,
            provider_type: "github".to_string(),
            name: name.to_string(),
            status,
            last_run: Some(Utc::now()),
            last_updated: Utc::now(),
            repository: "test/repo".to_string(),
            branch: Some("main".to_string()),
            workflow_file: Some("ci.yml".to_string()),
            pinned: true,
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn test_empty_pipelines() {
        let summary = TrayStatusSummary::from_pipelines(&[]);
        assert_eq!(summary.overall_status, TrayStatus::Unknown);
        assert!(!summary.has_pinned_pipelines());
    }

    #[test]
    fn test_all_passed() {
        let pipelines = vec![
            make_pipeline("1", "Build", PipelineStatus::Success),
            make_pipeline("2", "Test", PipelineStatus::Success),
        ];
        let summary = TrayStatusSummary::from_pipelines(&pipelines);
        assert_eq!(summary.overall_status, TrayStatus::Passed);
        assert_eq!(summary.passed_count, 2);
    }

    #[test]
    fn test_running_takes_priority() {
        let pipelines = vec![
            make_pipeline("1", "Build", PipelineStatus::Success),
            make_pipeline("2", "Test", PipelineStatus::Running),
            make_pipeline("3", "Deploy", PipelineStatus::Failed),
        ];
        let summary = TrayStatusSummary::from_pipelines(&pipelines);
        assert_eq!(summary.overall_status, TrayStatus::Running);
    }

    #[test]
    fn test_failed_over_passed() {
        let pipelines = vec![
            make_pipeline("1", "Build", PipelineStatus::Success),
            make_pipeline("2", "Test", PipelineStatus::Failed),
        ];
        let summary = TrayStatusSummary::from_pipelines(&pipelines);
        assert_eq!(summary.overall_status, TrayStatus::Failed);
    }
}
