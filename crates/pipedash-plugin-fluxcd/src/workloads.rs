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

pub(crate) struct Finding {
    pub owner: Target,
    pub workload: String,
    pub message: String,
    settled: bool,
    min_ready: Duration,
}

pub(crate) struct UnhealthyWorkloads {
    grace: Duration,
    since: HashMap<String, Instant>,
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

    pub(crate) fn lasting(&mut self, findings: Vec<Finding>, now: Instant) -> Vec<Finding> {
        let previous = std::mem::take(&mut self.since);
        findings
            .into_iter()
            .filter(|finding| {
                let first = previous.get(&finding.workload).copied().unwrap_or(now);
                self.since.insert(finding.workload.clone(), first);
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
        settled: issue.settled,
        min_ready: Duration::from_secs(document.spec.min_ready_seconds),
    }))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn workload(spec: Value, status: Value) -> Value {
        json!({
            "metadata": {
                "name": "web",
                "namespace": "apps",
                "generation": 4,
                "labels": {
                    "kustomize.toolkit.fluxcd.io/name": "app",
                    "kustomize.toolkit.fluxcd.io/namespace": "ci",
                },
            },
            "spec": spec,
            "status": status,
        })
    }

    fn found(kind: WorkloadKind, spec: Value, status: Value) -> Option<Finding> {
        finding(kind, Kind::Kustomization, workload(spec, status)).unwrap()
    }

    fn issue(kind: WorkloadKind, spec: Value, status: Value) -> Option<String> {
        found(kind, spec, status).map(|found| found.message)
    }

    fn warming_daemon_set(min_ready_seconds: u64) -> Finding {
        found(
            WorkloadKind::DaemonSet,
            json!({ "minReadySeconds": min_ready_seconds }),
            json!({
                "observedGeneration": 4, "desiredNumberScheduled": 3,
                "updatedNumberScheduled": 3, "numberUnavailable": 1,
            }),
        )
        .unwrap()
    }

    fn messages(findings: Vec<Finding>) -> Vec<String> {
        findings.into_iter().map(|found| found.message).collect()
    }

    fn progressing(status: &str, reason: &str) -> Value {
        json!({ "type": "Progressing", "status": status, "reason": reason })
    }

    #[test]
    fn deployment_is_unhealthy_when_fully_rolled_out_but_replicas_are_not_ready() {
        let status = json!({
            "observedGeneration": 4, "replicas": 3, "updatedReplicas": 3,
            "readyReplicas": 1, "availableReplicas": 1,
            "conditions": [progressing("True", "NewReplicaSetAvailable")],
        });

        assert_eq!(
            issue(WorkloadKind::Deployment, json!({ "replicas": 3 }), status).as_deref(),
            Some("Deployment apps/web: 1 of 3 replicas ready")
        );
    }

    #[test]
    fn deployment_with_no_ready_replicas_and_no_status_counts_is_unhealthy() {
        let status = json!({ "observedGeneration": 4, "updatedReplicas": 1 });

        assert_eq!(
            issue(WorkloadKind::Deployment, json!({}), status).as_deref(),
            Some("Deployment apps/web: 0 of 1 replicas ready")
        );
    }

    #[test]
    fn deployment_rollouts_in_progress_are_not_unhealthy() {
        let cases = [
            json!({ "observedGeneration": 3, "updatedReplicas": 3, "readyReplicas": 0 }),
            json!({ "observedGeneration": 4, "updatedReplicas": 1, "readyReplicas": 2 }),
            json!({
                "observedGeneration": 4, "updatedReplicas": 3, "readyReplicas": 0,
                "conditions": [progressing("True", "ReplicaSetUpdated")],
            }),
        ];

        for status in cases {
            assert_eq!(
                issue(WorkloadKind::Deployment, json!({ "replicas": 3 }), status),
                None
            );
        }
    }

    #[test]
    fn deployment_past_its_progress_deadline_is_unhealthy_even_mid_rollout() {
        let status = json!({
            "observedGeneration": 4, "updatedReplicas": 1, "readyReplicas": 3,
            "conditions": [progressing("False", "ProgressDeadlineExceeded")],
        });

        assert_eq!(
            issue(WorkloadKind::Deployment, json!({ "replicas": 3 }), status).as_deref(),
            Some("Deployment apps/web: rollout exceeded its progress deadline")
        );
    }

    #[test]
    fn scaled_to_zero_workloads_are_healthy() {
        let status = json!({
            "observedGeneration": 4, "updatedReplicas": 0, "readyReplicas": 0,
            "currentRevision": "web-1", "updateRevision": "web-1",
            "conditions": [progressing("False", "ProgressDeadlineExceeded")],
        });

        for kind in [WorkloadKind::Deployment, WorkloadKind::StatefulSet] {
            assert_eq!(issue(kind, json!({ "replicas": 0 }), status.clone()), None);
        }
    }

    #[test]
    fn stateful_set_is_unhealthy_only_when_no_update_is_pending() {
        let steady = json!({
            "observedGeneration": 4, "readyReplicas": 1,
            "currentRevision": "web-1", "updateRevision": "web-1",
        });
        let updating = json!({
            "observedGeneration": 4, "readyReplicas": 1,
            "currentRevision": "web-1", "updateRevision": "web-2",
        });
        let fresh = json!({ "observedGeneration": 4, "readyReplicas": 0 });
        let spec = json!({ "replicas": 3 });

        assert_eq!(
            issue(WorkloadKind::StatefulSet, spec.clone(), steady).as_deref(),
            Some("StatefulSet apps/web: 1 of 3 replicas ready")
        );
        assert_eq!(
            issue(WorkloadKind::StatefulSet, spec.clone(), updating),
            None
        );
        assert_eq!(issue(WorkloadKind::StatefulSet, spec, fresh), None);
    }

    #[test]
    fn daemon_set_is_unhealthy_only_when_pods_are_unavailable_after_the_update() {
        let unavailable = json!({
            "observedGeneration": 4, "desiredNumberScheduled": 5,
            "updatedNumberScheduled": 5, "numberUnavailable": 2,
        });
        let updating = json!({
            "observedGeneration": 4, "desiredNumberScheduled": 5,
            "updatedNumberScheduled": 2, "numberUnavailable": 2,
        });
        let healthy = json!({
            "observedGeneration": 4, "desiredNumberScheduled": 5,
            "updatedNumberScheduled": 5, "numberUnavailable": 0,
        });

        assert_eq!(
            issue(WorkloadKind::DaemonSet, json!({}), unavailable).as_deref(),
            Some("DaemonSet apps/web: 3 of 5 pods available")
        );
        assert_eq!(issue(WorkloadKind::DaemonSet, json!({}), updating), None);
        assert_eq!(issue(WorkloadKind::DaemonSet, json!({}), healthy), None);
    }

    #[test]
    fn a_stale_observed_generation_is_never_reported() {
        let status = json!({ "observedGeneration": 3, "updatedReplicas": 3, "readyReplicas": 0 });

        assert_eq!(
            issue(WorkloadKind::Deployment, json!({ "replicas": 3 }), status),
            None
        );
    }

    #[test]
    fn finding_is_attributed_to_the_owner_named_by_the_flux_labels() {
        let status = json!({ "observedGeneration": 4, "updatedReplicas": 1 });
        let item = workload(json!({}), status);

        let found = finding(WorkloadKind::Deployment, Kind::Kustomization, item.clone())
            .unwrap()
            .unwrap();
        assert_eq!(
            found.owner,
            Target::new(Kind::Kustomization, "ci", "app").unwrap()
        );
        assert!(finding(WorkloadKind::Deployment, Kind::HelmRelease, item)
            .unwrap()
            .is_none());
    }

    #[test]
    fn unhealthy_workloads_are_reported_only_after_they_stay_unhealthy() {
        let start = Instant::now();
        let mut tracker = UnhealthyWorkloads::default();

        assert!(tracker
            .lasting(vec![warming_daemon_set(0)], start)
            .is_empty());
        assert!(tracker
            .lasting(vec![warming_daemon_set(0)], start + Duration::from_secs(90))
            .is_empty());
        assert_eq!(
            messages(tracker.lasting(vec![warming_daemon_set(0)], start + UNSETTLED_GRACE)),
            ["DaemonSet apps/web: 2 of 3 pods available"]
        );
    }

    #[test]
    fn min_ready_seconds_extend_the_wait_before_reporting() {
        let start = Instant::now();
        let mut tracker = UnhealthyWorkloads::default();
        tracker.lasting(vec![warming_daemon_set(300)], start);

        assert!(tracker
            .lasting(vec![warming_daemon_set(300)], start + UNSETTLED_GRACE)
            .is_empty());
        assert_eq!(
            tracker
                .lasting(
                    vec![warming_daemon_set(300)],
                    start + UNSETTLED_GRACE + Duration::from_secs(300),
                )
                .len(),
            1
        );
    }

    #[test]
    fn recovering_restarts_the_wait() {
        let start = Instant::now();
        let mut tracker = UnhealthyWorkloads::default();
        tracker.lasting(vec![warming_daemon_set(0)], start);
        tracker.lasting(Vec::new(), start + Duration::from_secs(60));

        assert!(tracker
            .lasting(vec![warming_daemon_set(0)], start + UNSETTLED_GRACE)
            .is_empty());
    }

    #[test]
    fn a_missed_progress_deadline_is_reported_at_once() {
        let deadline = found(
            WorkloadKind::Deployment,
            json!({ "replicas": 3 }),
            json!({
                "observedGeneration": 4, "updatedReplicas": 1, "readyReplicas": 3,
                "conditions": [progressing("False", "ProgressDeadlineExceeded")],
            }),
        )
        .unwrap();

        assert_eq!(
            UnhealthyWorkloads::default()
                .lasting(vec![deadline], Instant::now())
                .len(),
            1
        );
    }
}
