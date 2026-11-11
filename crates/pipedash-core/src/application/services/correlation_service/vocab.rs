pub(super) const RUN: &str = "run";
pub(super) const COMMIT: &str = "commit";
pub(super) const ARTIFACT: &str = "artifact";
pub(super) const UNIT: &str = "unit";
pub(super) const TARGET: &str = "target";
pub(super) const PIPELINE: &str = "pipeline";

pub(super) const BUILT_FROM: &str = "built_from";
pub(super) const PRODUCED: &str = "produced";
pub(super) const RUNS: &str = "runs";
pub(super) const SYNCED: &str = "synced";
pub(super) const INTRODUCED: &str = "introduced";
pub(super) const DEPLOYS_TO: &str = "deploys_to";
pub(super) const SAME_APPLICATION: &str = "same_application";
pub(super) const BUILDS: &str = "builds";

pub(super) const RUN_SOURCE_COMMIT: &str = "run_source_commit";
pub(super) const PROVIDER_PRODUCED: &str = "provider_produced";
pub(super) const ATTESTATION: &str = "attestation";
pub(super) const LOG_PUSH: &str = "log_push";
pub(super) const ATTESTATION_COMMIT: &str = "attestation_commit";
pub(super) const LABEL_CONFIRMED: &str = "label_confirmed";
pub(super) const UNIT_CONSUMED: &str = "unit_consumed";
pub(super) const UNIT_SOURCE_COMMIT: &str = "unit_source_commit";
pub(super) const CONFIG_LINEAGE: &str = "config_lineage";
pub(super) const UNIT_TARGET: &str = "unit_target";

pub(super) const LABEL_UNCONFIRMED: &str = "label_unconfirmed";
pub(super) const TAG_SHA: &str = "tag_sha";
pub(super) const SAME_ARTIFACT_REPO_DIGEST: &str = "same_artifact_repo_digest";
pub(super) const SAME_REPO_PATH: &str = "same_repo_path";
pub(super) const SAME_LABEL: &str = "same_label";
pub(super) const SAME_NAME: &str = "same_name";

pub(super) const ORIGIN_PROVIDER: &str = "provider";
pub(super) const ORIGIN_LOG: &str = "log";
pub(super) const ORIGIN_ATTESTATION: &str = "attestation";

pub(super) const REL_PRODUCED: &str = "produced";
pub(super) const REL_CONSUMED: &str = "consumed";

pub(super) const LIFETIME_STANDING: &str = "standing";
pub(super) const LIFETIME_SHORT_LIVED: &str = "short_lived";

pub(super) fn run_node_id(pipeline_id: &str, run_number: i64) -> String {
    format!("{pipeline_id}#{run_number}")
}

pub(super) fn parse_run_node(node: &str) -> Option<(&str, i64)> {
    let (pipeline, number) = node.rsplit_once('#')?;
    Some((pipeline, number.parse().ok()?))
}

pub(super) fn commit_node_id(repo: &str, sha: &str) -> String {
    format!("{repo}@{sha}")
}

pub(super) fn parse_commit_node(node: &str) -> Option<(&str, &str)> {
    node.rsplit_once('@')
}

pub(super) fn unit_node_id(provider_id: i64, local_unit: &str) -> String {
    format!("{provider_id}:{local_unit}")
}

pub(super) fn target_node_id(cluster: &str, namespace: Option<&str>) -> String {
    format!("{cluster}|{}", namespace.unwrap_or(""))
}
