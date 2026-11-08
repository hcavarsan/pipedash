use std::collections::HashMap;
use std::time::{
    Duration,
    Instant,
};

use pipedash_plugin_api::{
    PluginError,
    PluginResult,
};
use serde::Deserialize;
use serde_json::Value;

use crate::{
    config::Target,
    types::Kind,
};

const PROGRESS_DEADLINE_EXCEEDED: &str = "ProgressDeadlineExceeded";
const REPLICA_SET_UPDATED: &str = "ReplicaSetUpdated";
const UNSETTLED_GRACE: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkloadKind {
    Deployment,
    StatefulSet,
    DaemonSet,
}

impl WorkloadKind {
    pub const ALL: [WorkloadKind; 3] = [
        WorkloadKind::Deployment,
        WorkloadKind::StatefulSet,
        WorkloadKind::DaemonSet,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            WorkloadKind::Deployment => "Deployment",
            WorkloadKind::StatefulSet => "StatefulSet",
            WorkloadKind::DaemonSet => "DaemonSet",
        }
    }

    pub const fn plural(self) -> &'static str {
        match self {
            WorkloadKind::Deployment => "deployments",
            WorkloadKind::StatefulSet => "statefulsets",
            WorkloadKind::DaemonSet => "daemonsets",
        }
    }

    pub fn collection_path(self) -> String {
        format!("/apis/apps/v1/{}", self.plural())
    }
}

pub(crate) fn owner_selector(owner: Kind) -> String {
    format!("{0}/name,{0}/namespace", owner.group())
}

#[derive(Default, Deserialize)]
struct Meta {
    #[serde(default)]
    name: String,
    #[serde(default)]
    namespace: String,
    #[serde(default)]
    generation: i64,
    #[serde(default)]
    labels: HashMap<String, String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    replicas: Option<i64>,
    #[serde(default)]
    min_ready_seconds: u64,
}

#[derive(Deserialize)]
struct Condition {
    #[serde(rename = "type")]
    kind: String,
    status: String,
    #[serde(default)]
    reason: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    #[serde(default)]
    observed_generation: i64,
    #[serde(default)]
    ready_replicas: i64,
    #[serde(default)]
    updated_replicas: i64,
    current_revision: Option<String>,
    update_revision: Option<String>,
    #[serde(default)]
    number_unavailable: i64,
    #[serde(default)]
    updated_number_scheduled: i64,
    #[serde(default)]
    desired_number_scheduled: i64,
    #[serde(default)]
    conditions: Vec<Condition>,
}

#[derive(Deserialize)]
struct Document {
    #[serde(default)]
    metadata: Meta,
    #[serde(default)]
    spec: Spec,
    #[serde(default)]
    status: Status,
}

fn replicas_ready(ready: i64, desired: i64) -> String {
    format!("{ready} of {desired} replicas ready")
}

struct Issue {
    detail: String,
    settled: bool,
}

impl Issue {
    fn settled(detail: &str) -> Self {
        Self {
            detail: detail.to_string(),
            settled: true,
        }
    }

    fn unsettled(detail: String) -> Self {
        Self {
            detail,
            settled: false,
        }
    }
}

impl Document {
    fn condition(&self, type_: &str) -> Option<&Condition> {
        self.status
            .conditions
            .iter()
            .find(|condition| condition.kind == type_)
    }

    fn deployment_issue(&self, desired: i64) -> Option<Issue> {
        let status = &self.status;
        let progressing = self.condition("Progressing");
        let reason = progressing.map_or("", |condition| condition.reason.as_str());

        if progressing.is_some_and(|condition| condition.status == "False")
            && reason == PROGRESS_DEADLINE_EXCEEDED
        {
            Some(Issue::settled("rollout exceeded its progress deadline"))
        } else if reason != REPLICA_SET_UPDATED
            && status.updated_replicas == desired
            && status.ready_replicas < desired
        {
            Some(Issue::unsettled(replicas_ready(
                status.ready_replicas,
                desired,
            )))
        } else {
            None
        }
    }

    fn stateful_set_issue(&self, desired: i64) -> Option<Issue> {
        let status = &self.status;

        (status.current_revision.is_some()
            && status.current_revision == status.update_revision
            && status.ready_replicas < desired)
            .then(|| Issue::unsettled(replicas_ready(status.ready_replicas, desired)))
    }

    fn daemon_set_issue(&self) -> Option<Issue> {
        let status = &self.status;

        (status.number_unavailable > 0
            && status.updated_number_scheduled == status.desired_number_scheduled)
            .then(|| {
                Issue::unsettled(format!(
                    "{} of {} pods available",
                    status.desired_number_scheduled - status.number_unavailable,
                    status.desired_number_scheduled
                ))
            })
    }

    fn issue(&self, kind: WorkloadKind) -> Option<Issue> {
        if self.status.observed_generation < self.metadata.generation {
            return None;
        }
        let desired = self.spec.replicas.unwrap_or(1);

        match kind {
            WorkloadKind::Deployment if desired > 0 => self.deployment_issue(desired),
            WorkloadKind::StatefulSet if desired > 0 => self.stateful_set_issue(desired),
            WorkloadKind::DaemonSet => self.daemon_set_issue(),
            _ => None,
        }
    }
}

pub(crate) type Lookup = (Kind, WorkloadKind);

pub(crate) struct Finding {
    pub owner: Target,
    pub workload: String,
    pub message: String,
    kind: WorkloadKind,
    settled: bool,
    min_ready: Duration,
}

impl Finding {
    fn lookup(&self) -> Lookup {
        (self.owner.kind, self.kind)
    }
}

pub(crate) struct UnhealthyWorkloads {
    grace: Duration,
    since: HashMap<String, (Lookup, Instant)>,
}

impl Default for UnhealthyWorkloads {
    fn default() -> Self {
        Self::with_grace(UNSETTLED_GRACE)
    }
}

impl UnhealthyWorkloads {
    pub(crate) fn with_grace(grace: Duration) -> Self {
        Self {
            grace,
            since: HashMap::new(),
        }
    }

    pub(crate) fn lasting(
        &mut self, findings: Vec<Finding>, failed: &[Lookup], now: Instant,
    ) -> Vec<Finding> {
        let previous = std::mem::take(&mut self.since);
        self.since.extend(
            previous
                .iter()
                .filter(|(_, (lookup, _))| failed.contains(lookup))
                .map(|(workload, seen)| (workload.clone(), *seen)),
        );
        findings
            .into_iter()
            .filter(|finding| {
                let first = previous
                    .get(&finding.workload)
                    .map_or(now, |(_, first)| *first);
                self.since
                    .insert(finding.workload.clone(), (finding.lookup(), first));
                finding.settled || now.duration_since(first) >= self.grace + finding.min_ready
            })
            .collect()
    }
}

pub(crate) fn finding(
    kind: WorkloadKind, owner: Kind, value: Value,
) -> PluginResult<Option<Finding>> {
    let document: Document = serde_json::from_value(value).map_err(|error| {
        PluginError::SerializationError(format!("Failed to parse {}: {error}", kind.as_str()))
    })?;
    let group = owner.group();
    let labels = &document.metadata.labels;
    let (Some(name), Some(namespace), Some(issue)) = (
        labels.get(&format!("{group}/name")),
        labels.get(&format!("{group}/namespace")),
        document.issue(kind),
    ) else {
        return Ok(None);
    };
    let workload = format!(
        "{} {}/{}",
        kind.as_str(),
        document.metadata.namespace,
        document.metadata.name
    );

    Ok(Some(Finding {
        owner: Target {
            kind: owner,
            namespace: namespace.clone(),
            name: name.clone(),
        },
        message: format!("{workload}: {}", issue.detail),
        workload,
        kind,
        settled: issue.settled,
        min_ready: Duration::from_secs(document.spec.min_ready_seconds),
    }))
}

#[cfg(test)]
mod tests;
