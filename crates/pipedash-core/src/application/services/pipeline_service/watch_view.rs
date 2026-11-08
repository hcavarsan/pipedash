use pipedash_plugin_api::build_watch::{
    BuildWatchMonitoring,
    BuildWatchRunStatus,
    BuildWatchSnapshot,
};

use super::WatchedRun;
use crate::domain::{
    DomainError,
    DomainResult,
    PipelineStatus,
};

pub fn validate_watch_selection(pipeline_id: &str, run_number: i64) -> DomainResult<()> {
    if pipeline_id.trim().is_empty()
        || pipeline_id.len() > 2048
        || pipeline_id.chars().any(char::is_control)
        || !(1..=9_007_199_254_740_991).contains(&run_number)
    {
        return Err(DomainError::InvalidConfig(
            "Choose a valid pipeline and a positive, safe run number".to_string(),
        ));
    }
    Ok(())
}

pub fn watch_snapshot(
    session_id: &str, pipeline_id: &str, run_number: i64, watched: WatchedRun,
) -> DomainResult<BuildWatchSnapshot> {
    let WatchedRun {
        pipeline,
        provider_name,
        run,
        observed_at,
    } = watched;
    if pipeline.id != pipeline_id || run.pipeline_id != pipeline.id || run.run_number != run_number
    {
        return Err(DomainError::InvalidConfig(
            "The provider returned a different pipeline or build run".to_string(),
        ));
    }
    let status = match run.status {
        PipelineStatus::Pending => BuildWatchRunStatus::Pending,
        PipelineStatus::Running => BuildWatchRunStatus::Running,
        PipelineStatus::Success => BuildWatchRunStatus::Success,
        PipelineStatus::Failed | PipelineStatus::Degraded => BuildWatchRunStatus::Failed,
        PipelineStatus::Cancelled => BuildWatchRunStatus::Cancelled,
        PipelineStatus::Skipped => BuildWatchRunStatus::Skipped,
    };
    let url = BuildWatchSnapshot::run_url(&pipeline.id, run_number);
    let watch = BuildWatchSnapshot {
        session_id: session_id.to_string(),
        pipeline_id: pipeline.id,
        pipeline_name: pipeline.name,
        provider_name,
        provider_type: pipeline.provider_type,
        run_number,
        branch: run.branch,
        status,
        started_at: run.started_at.timestamp(),
        concluded_at: run.concluded_at.map(|time| time.timestamp()),
        updated_at: observed_at.timestamp(),
        monitoring: if status.is_terminal() {
            BuildWatchMonitoring::Completed
        } else {
            BuildWatchMonitoring::Watching
        },
        url,
    };
    watch.validate().map_err(DomainError::InvalidConfig)?;
    Ok(watch)
}

#[cfg(test)]
mod tests;
