use std::collections::HashMap;

use pipedash_plugin_api::plugin::defaults;
use pipedash_plugin_api::{
    ActionAvailability,
    ActionIcon,
    PipelineActionState,
    PipelineFacts,
    PluginCapabilities,
    RerunStrategy,
    RunActionState,
    RunDisplay,
    RunFacts,
};
use serde_json::Value;

const MISSING_REASON: &str =
    "This resource was not found. Update the selection or recreate it in Flux.";
const SUSPENDED_REASON: &str =
    "This resource is suspended. Resume it in Flux before requesting reconciliation.";
const DEFAULT_RUN_LABEL: &str = "Reconciliation";

fn flag(metadata: &HashMap<String, Value>, key: &str) -> bool {
    metadata.get(key) == Some(&Value::Bool(true))
}

fn block_reason(metadata: &HashMap<String, Value>) -> Option<&'static str> {
    if flag(metadata, "missing") {
        Some(MISSING_REASON)
    } else if flag(metadata, "suspended") {
        Some(SUSPENDED_REASON)
    } else {
        None
    }
}

pub(crate) fn pipeline_actions(
    capabilities: &PluginCapabilities, pipeline: &PipelineFacts<'_>,
) -> PipelineActionState {
    let mut state = defaults::pipeline_actions(capabilities, pipeline);
    state.rerun_strategy = RerunStrategy::ReconcileCurrent;
    state.trigger_icon = ActionIcon::Refresh;
    if let Some(reason) = block_reason(pipeline.metadata) {
        state.trigger = ActionAvailability::blocked(reason);
    }
    state
}

pub(crate) fn run_actions(
    capabilities: &PluginCapabilities, pipeline: &PipelineFacts<'_>, run: &RunFacts<'_>,
) -> RunActionState {
    let mut state = defaults::run_actions(capabilities, run);
    if let Some(reason) = block_reason(pipeline.metadata) {
        state.rerun = ActionAvailability::blocked(reason);
    }
    state
}

pub(crate) fn run_display(run: &RunFacts<'_>) -> RunDisplay {
    let title = run
        .metadata
        .get("run_label")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_RUN_LABEL);
    let message = run
        .metadata
        .get("message")
        .and_then(Value::as_str)
        .or(run.commit_message)
        .filter(|message| !message.is_empty());

    RunDisplay {
        title: title.to_string(),
        message: message.map(str::to_string),
        context: Vec::new(),
        started_label: "Recorded".to_string(),
    }
}

#[cfg(test)]
mod tests;
