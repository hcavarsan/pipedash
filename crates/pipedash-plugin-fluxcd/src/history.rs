use std::collections::HashMap;

use pipedash_plugin_api::PipelineStatus;
use serde_json::Value;

use crate::mapper::history::RECORD_HISTORY;

pub(crate) fn can_archive_run(
    pipeline_metadata: &HashMap<String, Value>, run_metadata: &HashMap<String, Value>,
    status: PipelineStatus,
) -> bool {
    let Some(uid) = pipeline_metadata.get("uid").and_then(Value::as_str) else {
        return false;
    };

    run_metadata.get("uid").and_then(Value::as_str) == Some(uid)
        && run_metadata.get("record_source").and_then(Value::as_str) == Some(RECORD_HISTORY)
        && !matches!(
            status,
            PipelineStatus::Running | PipelineStatus::Pending | PipelineStatus::Degraded
        )
}

#[cfg(test)]
mod tests;
