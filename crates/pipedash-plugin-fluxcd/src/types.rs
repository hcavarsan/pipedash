use std::{
    collections::HashMap,
    fmt,
};

use chrono::{
    DateTime,
    Utc,
};
use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Kind {
    Kustomization,
    HelmRelease,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Kustomization, Kind::HelmRelease];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Kustomization => "Kustomization",
            Kind::HelmRelease => "HelmRelease",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == name)
    }

    pub const fn group(self) -> &'static str {
        match self {
            Kind::Kustomization => "kustomize.toolkit.fluxcd.io",
            Kind::HelmRelease => "helm.toolkit.fluxcd.io",
        }
    }

    pub fn version(self) -> &'static str {
        match self {
            Kind::Kustomization => "v1",
            Kind::HelmRelease => "v2",
        }
    }

    pub const fn plural(self) -> &'static str {
        match self {
            Kind::Kustomization => "kustomizations",
            Kind::HelmRelease => "helmreleases",
        }
    }

    pub fn collection_path(self, namespace: Option<&str>) -> String {
        match namespace {
            Some(namespace) => format!(
                "/apis/{}/{}/namespaces/{}/{}",
                self.group(),
                self.version(),
                namespace,
                self.plural()
            ),
            None => format!(
                "/apis/{}/{}/{}",
                self.group(),
                self.version(),
                self.plural()
            ),
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObjectMeta {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub namespace: String,
    pub uid: Option<String>,
    #[serde(default)]
    pub generation: i64,
    pub creation_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    pub annotations: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct Reference {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChartSpec {
    #[serde(default)]
    pub chart: String,
    pub version: Option<String>,
    pub source_ref: Option<Reference>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct ChartTemplate {
    #[serde(default)]
    pub spec: ChartSpec,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Spec {
    pub suspend: Option<bool>,
    pub interval: Option<String>,
    pub path: Option<String>,
    pub source_ref: Option<Reference>,
    pub chart: Option<ChartTemplate>,
    pub chart_ref: Option<Reference>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Condition {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub message: String,
    pub last_transition_time: Option<DateTime<Utc>>,
    pub observed_generation: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HistoryEntry {
    pub digest: Option<String>,
    pub first_reconciled: Option<DateTime<Utc>>,
    pub last_reconciled: Option<DateTime<Utc>>,
    pub last_reconciled_duration: Option<String>,
    pub last_reconciled_status: Option<String>,
    pub total_reconciliations: Option<i64>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    pub action: Option<String>,
    pub chart_name: Option<String>,
    pub chart_version: Option<String>,
    pub app_version: Option<String>,
    pub config_digest: Option<String>,
    pub first_deployed: Option<DateTime<Utc>>,
    pub last_deployed: Option<DateTime<Utc>>,
    pub status: Option<String>,
    pub version: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Status {
    #[serde(default)]
    pub conditions: Vec<Condition>,
    pub observed_generation: Option<i64>,
    pub last_attempted_revision: Option<String>,
    pub last_applied_revision: Option<String>,
    pub last_handled_reconcile_at: Option<String>,
    pub last_attempted_release_action: Option<String>,
    pub last_attempted_release_action_duration: Option<String>,
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
}

#[derive(Debug, Clone)]
pub(crate) struct FluxResource {
    pub kind: Kind,
    pub metadata: ObjectMeta,
    pub spec: Spec,
    pub status: Status,
}

#[derive(Deserialize)]
struct Document {
    #[serde(default)]
    metadata: ObjectMeta,
    #[serde(default)]
    spec: Spec,
    #[serde(default)]
    status: Status,
}

fn string_at<'a>(value: &'a Value, pointer: &str) -> &'a str {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or("?")
}

impl FluxResource {
    pub fn from_value(kind: Kind, value: Value) -> PluginResult<Self> {
        let document = Document::deserialize(&value).map_err(|error| {
            PluginError::SerializationError(format!(
                "Failed to parse {kind} {}/{}: {error}",
                string_at(&value, "/metadata/namespace"),
                string_at(&value, "/metadata/name")
            ))
        })?;

        Ok(Self {
            kind,
            metadata: document.metadata,
            spec: document.spec,
            status: document.status,
        })
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EventSeries {
    pub count: Option<i64>,
    pub last_observed_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct EventSource {
    pub component: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Event {
    #[serde(rename = "type", default)]
    pub type_: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub message: String,
    pub count: Option<i64>,
    pub first_timestamp: Option<DateTime<Utc>>,
    pub last_timestamp: Option<DateTime<Utc>>,
    pub event_time: Option<DateTime<Utc>>,
    pub series: Option<EventSeries>,
    pub source: Option<EventSource>,
    pub reporting_component: Option<String>,
}

impl Event {
    pub fn time(&self) -> Option<DateTime<Utc>> {
        self.series
            .as_ref()
            .and_then(|series| series.last_observed_time)
            .or(self.last_timestamp)
            .or(self.event_time)
            .or(self.first_timestamp)
    }

    pub fn occurrences(&self) -> i64 {
        self.series
            .as_ref()
            .and_then(|series| series.count)
            .or(self.count)
            .unwrap_or(1)
    }

    pub fn component(&self) -> Option<&str> {
        self.source
            .as_ref()
            .and_then(|source| source.component.as_deref())
            .or(self.reporting_component.as_deref())
            .filter(|component| !component.is_empty())
    }
}
