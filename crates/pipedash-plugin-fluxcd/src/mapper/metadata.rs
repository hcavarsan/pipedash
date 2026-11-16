use serde_json::Value;

use super::state::{
    asserted,
    condition,
    is_stale,
    is_suspended,
};
use super::Metadata;
use crate::client::REQUESTED_AT_ANNOTATION;
use crate::types::{
    FluxResource,
    Kind,
    Reference,
};

const HISTORY_SCOPE: &str = "bounded_reconciliation";

pub(super) fn put<V: Into<Value>>(metadata: &mut Metadata, key: &str, value: Option<V>) {
    if let Some(value) = value {
        metadata.insert(key.to_string(), value.into());
    }
}

pub(super) fn base_metadata(resource: &FluxResource) -> Metadata {
    let mut metadata = Metadata::new();
    put(&mut metadata, "kind", Some(resource.kind.as_str()));
    put(
        &mut metadata,
        "namespace",
        Some(resource.metadata.namespace.as_str()),
    );
    put(&mut metadata, "name", Some(resource.metadata.name.as_str()));
    put(&mut metadata, "uid", resource.metadata.uid.as_deref());
    put(&mut metadata, "history_scope", Some(HISTORY_SCOPE));
    metadata
}

pub(super) fn source_reference(resource: &FluxResource) -> Option<&Reference> {
    match resource.kind {
        Kind::Kustomization => resource.spec.source_ref.as_ref(),
        Kind::HelmRelease => resource
            .spec
            .chart
            .as_ref()
            .and_then(|chart| chart.spec.source_ref.as_ref())
            .or(resource.spec.chart_ref.as_ref()),
    }
    .filter(|reference| !reference.name.is_empty())
}

pub(super) fn source_ref(resource: &FluxResource) -> Option<String> {
    source_reference(resource).map(|reference| {
        format!(
            "{}/{}/{}",
            reference.kind,
            reference
                .namespace
                .as_deref()
                .unwrap_or(&resource.metadata.namespace),
            reference.name
        )
    })
}

fn chart(resource: &FluxResource) -> Option<String> {
    if resource.kind != Kind::HelmRelease {
        return None;
    }

    match resource.spec.chart.as_ref() {
        Some(template) if !template.spec.chart.is_empty() => {
            Some(match template.spec.version.as_deref() {
                Some(version) if !version.is_empty() => {
                    format!("{}@{version}", template.spec.chart)
                }
                _ => template.spec.chart.clone(),
            })
        }
        _ => resource
            .spec
            .chart_ref
            .as_ref()
            .map(|reference| reference.name.clone()),
    }
}

pub(super) fn current_revision(resource: &FluxResource) -> Option<&str> {
    resource
        .status
        .last_attempted_revision
        .as_deref()
        .or(resource.status.last_applied_revision.as_deref())
}

pub(super) fn latest_release_version(resource: &FluxResource) -> Option<i64> {
    resource
        .status
        .history
        .iter()
        .filter_map(|entry| entry.version)
        .max()
}

fn trim_algorithm(digest: &str) -> &str {
    digest.rsplit_once(':').map_or(digest, |(_, hex)| hex)
}

pub(super) fn split_revision(revision: &str) -> (Option<&str>, Option<&str>) {
    match revision.split_once('@') {
        Some((reference, digest)) => (
            Some(reference).filter(|reference| !reference.is_empty()),
            Some(trim_algorithm(digest)).filter(|digest| !digest.is_empty()),
        ),
        None => (None, revision.rsplit_once(':').map(|(_, hex)| hex)),
    }
}

pub(super) fn display_name(resource: &FluxResource) -> String {
    format!("{}/{}", resource.kind, resource.metadata.name)
}

pub(super) fn repository(resource: &FluxResource) -> String {
    match source_reference(resource) {
        Some(reference) => format!("{}/{}", resource.metadata.namespace, reference.name),
        None => resource.metadata.namespace.clone(),
    }
}

pub(super) fn workflow_file(resource: &FluxResource) -> Option<String> {
    match resource.kind {
        Kind::Kustomization => resource.spec.path.clone().filter(|path| !path.is_empty()),
        Kind::HelmRelease => chart(resource),
    }
}

pub(super) fn state_metadata(resource: &FluxResource, reason: String) -> Metadata {
    let ready = condition(resource, "Ready");
    let mut metadata = Metadata::new();
    put(&mut metadata, "suspended", Some(is_suspended(resource)));
    put(&mut metadata, "stale", Some(is_stale(resource)));
    put(&mut metadata, "reason", Some(reason));
    put(
        &mut metadata,
        "ready_status",
        ready.map(|ready| ready.status.as_str()),
    );
    put(
        &mut metadata,
        "ready_reason",
        ready.map(|ready| ready.reason.as_str()),
    );
    put(
        &mut metadata,
        "ready_message",
        ready.map(|ready| ready.message.as_str()),
    );
    put(
        &mut metadata,
        "reconciling_reason",
        asserted(resource, "Reconciling").map(|reconciling| reconciling.reason.as_str()),
    );
    put(
        &mut metadata,
        "stalled_reason",
        asserted(resource, "Stalled").map(|stalled| stalled.reason.as_str()),
    );
    put(
        &mut metadata,
        "generation",
        Some(resource.metadata.generation),
    );
    put(
        &mut metadata,
        "observed_generation",
        resource.status.observed_generation,
    );
    put(&mut metadata, "revision", current_revision(resource));
    put(
        &mut metadata,
        "applied_revision",
        resource.status.last_applied_revision.as_deref(),
    );
    put(&mut metadata, "source_ref", source_ref(resource));
    put(&mut metadata, "chart", chart(resource));
    put(&mut metadata, "interval", resource.spec.interval.as_deref());
    put(
        &mut metadata,
        "release_version",
        latest_release_version(resource),
    );
    put(
        &mut metadata,
        "last_handled_reconcile_at",
        resource.status.last_handled_reconcile_at.as_deref(),
    );
    put(
        &mut metadata,
        "reconcile_requested_at",
        resource
            .metadata
            .annotations
            .get(REQUESTED_AT_ANNOTATION)
            .map(String::as_str),
    );
    metadata
}
