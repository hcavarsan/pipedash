use pipedash_plugin_api::PipelineStatus;

use crate::types::{
    Condition,
    FluxResource,
};

const PROGRESSING_WITH_RETRY: &str = "ProgressingWithRetry";

pub(super) fn condition<'a>(resource: &'a FluxResource, type_: &str) -> Option<&'a Condition> {
    resource
        .status
        .conditions
        .iter()
        .find(|condition| condition.type_ == type_)
}

fn is_fresh(resource: &FluxResource, condition: &Condition) -> bool {
    condition
        .observed_generation
        .or(resource.status.observed_generation)
        .is_none_or(|observed| observed >= resource.metadata.generation)
}

pub(super) fn fresh<'a>(resource: &'a FluxResource, type_: &str) -> Option<&'a Condition> {
    condition(resource, type_).filter(|condition| is_fresh(resource, condition))
}

pub(super) fn asserted<'a>(resource: &'a FluxResource, type_: &str) -> Option<&'a Condition> {
    fresh(resource, type_).filter(|condition| condition.status == "True")
}

pub(super) fn is_suspended(resource: &FluxResource) -> bool {
    resource.spec.suspend == Some(true)
}

pub(super) fn is_stale(resource: &FluxResource) -> bool {
    condition(resource, "Ready").is_some_and(|ready| !is_fresh(resource, ready))
}

pub(crate) fn current_status(resource: &FluxResource) -> PipelineStatus {
    if is_suspended(resource) {
        return PipelineStatus::Skipped;
    }

    let ready = fresh(resource, "Ready");
    let reconciling = asserted(resource, "Reconciling");
    let retrying_failure = ready.is_some_and(|ready| ready.status == "False")
        && reconciling.is_some_and(|reconciling| reconciling.reason == PROGRESSING_WITH_RETRY);

    if asserted(resource, "Stalled").is_some() || retrying_failure {
        PipelineStatus::Failed
    } else if reconciling.is_some() {
        PipelineStatus::Running
    } else {
        match ready.map(|ready| ready.status.as_str()) {
            Some("True") => PipelineStatus::Success,
            Some("False") => PipelineStatus::Failed,
            Some(_) => PipelineStatus::Running,
            None => PipelineStatus::Pending,
        }
    }
}

pub(crate) fn status_label(status: PipelineStatus) -> &'static str {
    match status {
        PipelineStatus::Success => "Ready",
        PipelineStatus::Failed => "Not ready",
        PipelineStatus::Degraded => "Degraded",
        PipelineStatus::Running => "Reconciling",
        PipelineStatus::Pending => "Pending",
        PipelineStatus::Skipped => "Suspended",
        PipelineStatus::Cancelled => "Cancelled",
    }
}

pub(crate) fn headline(resource: &FluxResource, status: PipelineStatus) -> (String, String) {
    if is_suspended(resource) {
        return (
            "Suspended".to_string(),
            "spec.suspend is true; Flux is not reconciling this resource".to_string(),
        );
    }
    if let Some(ready) = condition(resource, "Ready").filter(|ready| !is_fresh(resource, ready)) {
        let observed = ready
            .observed_generation
            .or(resource.status.observed_generation)
            .map_or_else(|| "unknown".to_string(), |observed| observed.to_string());
        return (
            "StatusOutdated".to_string(),
            format!(
                "Flux reported status for generation {observed}, but the resource is at generation {}",
                resource.metadata.generation
            ),
        );
    }

    let primary = match status {
        PipelineStatus::Running => {
            asserted(resource, "Reconciling").or_else(|| fresh(resource, "Ready"))
        }
        _ => fresh(resource, "Ready")
            .or_else(|| asserted(resource, "Stalled"))
            .or_else(|| asserted(resource, "Reconciling")),
    };

    match primary {
        Some(condition) => (condition.reason.clone(), condition.message.clone()),
        None => (
            "NoStatus".to_string(),
            "Flux has not reported status conditions yet".to_string(),
        ),
    }
}
